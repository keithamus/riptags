use std::path::Path;
use std::time::SystemTime;

use riptags::walk::Walk;
use riptags::{Cache, FileTags, Freshness, IndexOptions, SymbolIndex, cache};

mod support;
use support::{holds, wait_until, write};

fn tag(root: &Path, opts: &IndexOptions) -> Vec<FileTags> {
    let files = Walk::new(root, opts).files().unwrap();
    SymbolIndex::tag_files(files, opts).0
}

/// Every scenario lives in one test because `XDG_CACHE_HOME` is process-wide:
/// parallel test threads must not race to set it. Each scenario uses its own
/// root, so the cache files never collide.
#[test]
fn cache_roundtrips_deltas_versions_and_locks() {
    let home = tempfile::tempdir().unwrap();
    // SAFETY: this is the only test in this binary, so nothing else is
    // reading the environment yet.
    unsafe { std::env::set_var("XDG_CACHE_HOME", home.path()) };
    let opts = IndexOptions::default();

    // Roundtrip: what was stored comes back unchanged.
    let repo = tempfile::tempdir().unwrap();
    let cached = Cache::of(repo.path(), &opts);
    write(repo.path(), "src/lib.rs", "pub fn alpha() {}\n");
    let tagged = tag(repo.path(), &opts);
    assert_eq!(tagged.len(), 1);
    cached
        .store(&SymbolIndex::of_dir(repo.path(), &opts).unwrap())
        .unwrap();
    assert!(cached.path().exists());
    let loaded = cached.load().expect("a stored index maps");
    let file = loaded.files().next().unwrap();
    assert_eq!(file.path(), "src/lib.rs");
    assert_eq!(file.size(), tagged[0].size);
    assert_eq!(file.mtime_ns(), tagged[0].mtime_ns);
    let mapped: Vec<(String, u32, Vec<String>)> = file
        .tags()
        .map(|tag| {
            (
                tag.name.to_string(),
                tag.line,
                tag.owners().map(str::to_string).collect(),
            )
        })
        .collect();
    let parsed: Vec<(String, u32, Vec<String>)> = tagged[0]
        .tags
        .iter()
        .map(|tag| {
            (
                tag.name.to_string(),
                tag.line,
                tag.owners.iter().map(|owner| owner.to_string()).collect(),
            )
        })
        .collect();
    assert_eq!(mapped, parsed, "the map round-trips every tag");

    // Delta: only the files that changed are reparsed.
    write(
        repo.path(),
        "src/lib.rs",
        "pub fn alpha() {}\npub fn zz_delta_probe() {}\n",
    );
    write(repo.path(), "src/other.rs", "pub fn beta() {}\n");
    write(
        repo.path(),
        "src/owner.rs",
        "pub struct Thing;\nimpl Thing {\n    pub fn doit(&self) {}\n}\n",
    );
    let index = cached.resolve(&opts).unwrap().expect("the cache exists");
    assert_eq!(index.stats().files, 3);
    assert_eq!(
        index.stats().reused,
        0,
        "src/lib.rs changed and the others are new"
    );
    assert!(
        index.lookup("#zz_delta_probe", 10).is_some(),
        "the edit is visible"
    );
    assert!(index.drifted(), "the index on disk is behind the tree");
    cached.store(&index).unwrap();

    let index = cached.resolve(&opts).unwrap().unwrap();
    assert_eq!(
        index.stats().reused,
        3,
        "nothing changed, so nothing is reparsed"
    );
    assert!(!index.drifted(), "and nothing needs writing back");
    assert!(index.lookup("#beta", 10).is_some());
    let qualified = index
        .lookup("Thing#doit", 10)
        .expect("the owner-qualified symbol is in the map");
    assert_eq!(qualified.counts.definitions, 1);
    assert_eq!(
        index.lookup("#doit", 10).unwrap().counts.definitions,
        1,
        "the bare form finds it too"
    );
    assert!(
        index.lookup("Other#doit", 10).is_none(),
        "a symbol of another owner is not in the map"
    );

    // A same-length edit: the size cannot tell, so the mtime has to.
    let before = std::fs::metadata(repo.path().join("src/other.rs"))
        .unwrap()
        .len();
    write(repo.path(), "src/other.rs", "pub fn zeta() {}\n");
    assert_eq!(
        std::fs::metadata(repo.path().join("src/other.rs"))
            .unwrap()
            .len(),
        before,
        "the point of this scenario is that the size did not move"
    );
    let index = cached.resolve(&opts).unwrap().unwrap();
    assert_eq!(
        index.stats().reused,
        2,
        "only the rewritten file is reparsed"
    );
    assert!(
        index.lookup("#beta", 10).is_none() && index.lookup("#zeta", 10).is_some(),
        "an edit of the same byte length is still an edit"
    );
    cached.store(&index).unwrap();

    // Deletion: a removed file leaves the index.
    std::fs::remove_file(repo.path().join("src/other.rs")).unwrap();
    let index = cached.resolve(&opts).unwrap().unwrap();
    assert_eq!(index.stats().files, 2);
    assert!(
        index.lookup("#beta", 10).is_none(),
        "symbols of a deleted file are gone"
    );
    cached.store(&index).unwrap();
    assert_eq!(
        cached.load().unwrap().file_count(),
        2,
        "the shrunk index was written back"
    );

    // Build stamp mismatch: an index written by another build is not reused.
    let other = tempfile::tempdir().unwrap();
    let cached = Cache::of(other.path(), &opts);
    write(other.path(), "src/lib.rs", "pub fn alpha() {}\n");
    cached
        .store(&SymbolIndex::of_dir(other.path(), &opts).unwrap())
        .unwrap();
    assert!(cached.load().is_some());
    let path = cached.path().to_path_buf();
    let mut bytes = std::fs::read(&path).unwrap();
    // The stamp is the u64 at offset 8; flip it.
    bytes[8] ^= 0xff;
    std::fs::write(&path, bytes).unwrap();
    assert!(
        cached.load().is_none(),
        "an index from another build is discarded"
    );
    assert!(
        cached.resolve(&opts).unwrap().is_none(),
        "so the caller falls back to a cold scan"
    );

    // A truncated index is rejected rather than read out of bounds.
    let short = std::fs::read(&path).unwrap();
    std::fs::write(&path, &short[..short.len() / 2]).unwrap();
    assert!(cached.load().is_none(), "truncation is caught");

    // An index the caller keeps itself: written where it asked, mapped back
    // from there, and no cache entry for a directory nothing walked.
    let owned = tempfile::tempdir().unwrap();
    write(owned.path(), "src/lib.rs", "pub fn gamma() {}\n");
    let at = owned.path().join("snapshots/tags-c0ffee.idx");
    SymbolIndex::of_dir(owned.path(), &opts)
        .unwrap()
        .store_to(&at)
        .unwrap();
    assert!(
        !Cache::of(owned.path(), &opts).path().exists(),
        "storing to a path of my own leaves the cache alone"
    );
    assert!(
        SymbolIndex::mapped_at(&at)
            .expect("the index maps from where it was put")
            .lookup("#gamma", 10)
            .is_some()
    );
    assert!(SymbolIndex::mapped_at(&owned.path().join("nothing.idx")).is_none());

    // Freshness: the index answers as it stands until a caller asks for the
    // tree to be checked, which reparses the edit and writes it back.
    let live = tempfile::tempdir().unwrap();
    let cached = Cache::of(live.path(), &opts);
    write(live.path(), "src/lib.rs", "pub fn before() {}\n");
    let built = cached.query(&opts, Freshness::Index).unwrap();
    assert!(
        built.lookup("#before", 10).is_some(),
        "the first query builds the index"
    );
    assert!(cached.path().exists(), "and leaves it on disk");
    write(
        live.path(),
        "src/lib.rs",
        "pub fn before() {}\npub fn after() {}\n",
    );
    assert!(
        cached
            .query(&opts, Freshness::Index)
            .unwrap()
            .lookup("#after", 10)
            .is_none(),
        "a trusted answer does not see the edit"
    );
    assert!(
        cached
            .query(&opts, Freshness::Checked)
            .unwrap()
            .lookup("#after", 10)
            .is_some(),
        "a checked one does"
    );
    assert!(holds(live.path(), "#after"), "and stored what it parsed");

    // Refreshing: a tree nobody has checked lately is brought up to date on
    // a background thread.
    let stamp = std::fs::File::options()
        .write(true)
        .open(cached.path())
        .unwrap();
    stamp
        .set_modified(SystemTime::now() - 2 * cache::CHECK_EVERY)
        .unwrap();
    drop(stamp);
    write(
        live.path(),
        "src/lib.rs",
        "pub fn before() {}\npub fn after() {}\npub fn later() {}\n",
    );
    assert!(cached.refresh_due(), "nothing checked it lately");
    cached.refresh(&opts);
    wait_until("the background refresh", || holds(live.path(), "#later"));
    assert!(!cached.refresh_due(), "and the tree is checked again");

    // Locking: one builder at a time.
    let locked = tempfile::tempdir().unwrap();
    let cached = Cache::of(locked.path(), &opts);
    let held = cached.try_lock().expect("the first builder wins");
    assert!(
        cached.try_lock().is_none(),
        "a second builder must back off"
    );
    drop(held);
    assert!(
        cached.try_lock().is_some(),
        "the lock is released with the guard"
    );

    // Watching: one watcher at a time, and the claim dies with its holder.
    let watched = tempfile::tempdir().unwrap();
    let cached = Cache::of(watched.path(), &opts);
    assert!(cached.watcher().is_none(), "nobody yet");
    let claim = cached
        .claim_watch()
        .unwrap()
        .expect("the first watcher wins");
    assert_eq!(
        cached.watcher(),
        Some(std::process::id()),
        "and says who it is"
    );
    assert!(
        cached.claim_watch().unwrap().is_none(),
        "a second watcher must back off"
    );
    drop(claim);
    assert!(cached.watcher().is_none(), "the claim goes with its holder");
}
