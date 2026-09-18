#![cfg(all(feature = "rust", feature = "python"))]

use riptags::{IndexOptions, LangId, Scope, SearchHit, SymbolIndex};

mod support;
use support::source;

const SHARED_RS: &str = r#"pub fn handle() {}

pub fn rust_only() {
    handle();
}
"#;

const SHARED_PY: &str = r#"def handle():
    return None

def python_only():
    return handle()
"#;

fn syms(hits: &[SearchHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.sym.as_str()).collect()
}

fn mixed_index() -> SymbolIndex {
    SymbolIndex::build(
        vec![
            source("src/shared.rs", SHARED_RS),
            source("src/shared.py", SHARED_PY),
        ],
        &IndexOptions::default(),
    )
}

#[test]
fn a_language_narrows_a_listing_to_that_part_of_the_tree() {
    let index = mixed_index();

    let every = index.list(&[], Scope::default(), Some("handle"));
    assert_eq!(every[0].definitions, 2, "both languages define it");

    let python = index.list(&[], Scope::of(&[LangId::Python]), Some("handle"));
    assert_eq!(syms(&python), vec!["#handle"]);
    assert_eq!(
        python[0].definitions, 1,
        "a symbol counts only the occurrences of its language"
    );
    assert_eq!(
        python[0].references, 1,
        "the Python call counts, the Rust one does not"
    );

    let listed = index.list(&[], Scope::of(&[LangId::Rust]), None);
    let rust = syms(&listed);
    assert!(rust.contains(&"#rust_only"), "got {rust:?}");
    assert!(!rust.contains(&"#python_only"), "got {rust:?}");
}

#[test]
fn a_language_narrows_the_overview() {
    let index = mixed_index();

    let whole = index.overview(Scope::default());
    assert_eq!(whole.files, 2);
    assert_eq!(whole.languages.len(), 2);

    let python = index.overview(Scope::of(&[LangId::Python]));
    assert_eq!(python.files, 1);
    assert_eq!(python.languages.len(), 1);
    assert_eq!(python.languages[0].language, "python");
    assert!(python.occurrences > 0 && python.occurrences < whole.occurrences);
    assert!(python.symbols < whole.symbols);
}

/// The index on disk answers a language filter by tag slot rather than by
/// path, so it has to reach the same answer as a freshly parsed tree.
#[test]
fn a_mapped_index_filters_by_language_too() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.idx");
    mixed_index().store_to(&path).unwrap();
    let mapped = SymbolIndex::mapped_at(&path).expect("the index maps back");

    let python = mapped.list(&[], Scope::of(&[LangId::Python]), Some("handle"));
    assert_eq!(syms(&python), vec!["#handle"]);
    assert_eq!(python[0].definitions, 1);
    assert_eq!(python[0].references, 1);

    let listed = mapped.list(&[], Scope::of(&[LangId::Rust]), None);
    let rust = syms(&listed);
    assert!(!rust.contains(&"#python_only"), "got {rust:?}");

    assert_eq!(mapped.overview(Scope::of(&[LangId::Rust])).files, 1);
}
