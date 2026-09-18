#![allow(dead_code)]

use std::path::Path;
use std::process::Child;
use std::time::{Duration, Instant};

use riptags::{Cache, IndexOptions, OccKind, SourceFile, Tag};

pub fn find<'a>(tags: &'a [Tag], name: &str, kind: OccKind) -> Vec<&'a Tag> {
    tags.iter()
        .filter(|tag| &*tag.name == name && tag.kind == kind)
        .collect()
}

pub fn named<'a>(tags: &'a [Tag], name: &str) -> Vec<&'a Tag> {
    tags.iter().filter(|tag| &*tag.name == name).collect()
}

pub fn one<'a>(tags: &'a [Tag], name: &str, kind: OccKind) -> &'a Tag {
    let hits = find(tags, name, kind);
    assert_eq!(
        hits.len(),
        1,
        "expected one {kind:?} of `{name}`: {hits:#?}"
    );
    hits[0]
}

pub fn owners(tag: &Tag) -> Vec<&str> {
    tag.owners.iter().map(|owner| &**owner).collect()
}

pub fn source(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.to_string(),
        mtime_ns: 0,
        size: text.len() as u64,
        bytes: text.as_bytes().to_vec(),
    }
}

pub fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, text).unwrap();
}

/// Kills the watcher however the test ends.
pub struct Watcher(pub Child);

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn holds(root: &Path, sym: &str) -> bool {
    Cache::of(root, &IndexOptions::default())
        .mapped()
        .is_some_and(|index| index.lookup(sym, 1).is_some())
}

pub fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}
