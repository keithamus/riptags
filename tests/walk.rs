#![cfg(feature = "python")]

use riptags::IndexOptions;
use riptags::walk::Walk;

mod support;
use support::write;

#[test]
fn walks_indexable_files_only() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // `ignore` only applies .gitignore inside a repository.
    std::fs::create_dir_all(root.join(".git")).unwrap();
    write(root, ".gitignore", "ignored/\n");
    write(root, "a.rs", "pub fn alpha() {}\n");
    write(root, "sub/b.py", "def beta():\n    pass\n");
    write(root, "ignored/c.rs", "pub fn gamma() {}\n");
    write(root, "notes.md", "# nope\n");
    write(root, "big.rs", &"// pad\n".repeat(64));

    let opts = IndexOptions {
        max_file_bytes: 128,
        ..Default::default()
    };
    let mut paths: Vec<String> = Walk::new(root, &opts)
        .files()
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["a.rs".to_string(), "sub/b.py".to_string()]);

    let meta = Walk::new(root, &opts).meta().unwrap();
    assert_eq!(meta.len(), 2);
    assert!(
        meta.iter().all(|file| file.mtime_ns > 0 && file.size > 0),
        "change detection needs an mtime and a size: {meta:?}"
    );
}

#[test]
fn excluded_paths_never_reach_the_reader() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "src/a.rs", "pub fn alpha() {}\n");
    write(root, "vendor/b.rs", "pub fn beta() {}\n");

    let opts = IndexOptions {
        exclude: vec!["vendor".into()],
        ..Default::default()
    };
    let paths: Vec<String> = Walk::new(root, &opts)
        .files()
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert_eq!(paths, vec!["src/a.rs".to_string()]);
}

#[test]
fn a_missing_root_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        Walk::new(&dir.path().join("nope"), &IndexOptions::default())
            .files()
            .is_err()
    );
}
