use std::process::{Command, Stdio};

use riptags::{Cache, IndexOptions};

mod support;
use support::{Watcher, holds, wait_until, write};

/// The watcher owns the tree: edits land in the index without anyone asking,
/// and a second watcher on the same tree refuses to start rather than fight
/// the first one.
#[test]
fn a_watcher_keeps_the_index_current() {
    let home = tempfile::tempdir().unwrap();
    // SAFETY: this is the only test in this binary, so nothing else is
    // reading the environment yet.
    unsafe { std::env::set_var("XDG_CACHE_HOME", home.path()) };

    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    write(root, "src/lib.rs", "pub fn alpha() {}\n");

    let watcher = Watcher(
        Command::new(env!("CARGO_BIN_EXE_rt"))
            .args(["--watch", root.to_str().unwrap()])
            .env("XDG_CACHE_HOME", home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the watcher"),
    );
    wait_until("the first index", || holds(root, "#alpha"));

    let second = Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(["--watch", root.to_str().unwrap()])
        .env("XDG_CACHE_HOME", home.path())
        .output()
        .expect("run a second watcher");
    assert!(!second.status.success(), "a second watcher must refuse");
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("already watched"),
        "and say why: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    write(root, "src/added.rs", "pub fn zz_watch_probe() {}\n");
    wait_until("a new file to reach the index", || {
        holds(root, "#zz_watch_probe")
    });

    std::fs::remove_file(root.join("src/added.rs")).unwrap();
    wait_until("a deleted file to leave the index", || {
        !holds(root, "#zz_watch_probe")
    });

    drop(watcher);
    wait_until("the claim to be released", || {
        Cache::of(root, &IndexOptions::default())
            .watcher()
            .is_none()
    });
}
