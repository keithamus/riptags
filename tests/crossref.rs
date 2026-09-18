#![cfg(all(feature = "cpp", feature = "javascript"))]

use riptags::{IndexOptions, OccKind, Scope, SearchHit, SourceFile, SymKind, SymbolIndex};

mod support;
use support::source;

const ERROR_RS: &str = r#"pub struct ApiError {
    pub message: String,
}

impl ApiError {
    pub fn bad_request(message: &str) -> Self {
        ApiError { message: message.to_string() }
    }
}
"#;

const SERVER_RS: &str = r#"use crate::error::ApiError;

pub fn get_blob(path: &str) -> Result<String, ApiError> {
    if path.is_empty() {
        return Err(ApiError::bad_request("empty path"));
    }
    Err(ApiError::bad_request(path))
}

pub fn get_tree() -> Result<(), ApiError> {
    Err(ApiError::bad_request("nope"))
}
"#;

const FRAME_RS: &str = r#"pub fn send_frame() {}

pub fn try_send_frame() {
    send_frame();
}
"#;

const IGNORED_MD: &str = "# docs\n\nApiError::bad_request is documented here.\n";

fn fixture() -> Vec<SourceFile> {
    vec![
        source("src/error.rs", ERROR_RS),
        source("src/server.rs", SERVER_RS),
        source("README.md", IGNORED_MD),
        source("vendor/copy.rs", ERROR_RS),
    ]
}

fn build(opts: IndexOptions) -> SymbolIndex {
    SymbolIndex::build(fixture(), &opts)
}

fn without_vendor() -> IndexOptions {
    IndexOptions {
        exclude: vec!["vendor".into()],
        ..Default::default()
    }
}

fn syms(hits: &[SearchHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.sym.as_str()).collect()
}

fn search(index: &SymbolIndex, query: &str) -> Vec<String> {
    index
        .list(&[], Scope::default(), Some(query))
        .into_iter()
        .map(|hit| hit.sym)
        .collect()
}

#[test]
fn finds_definition_and_every_callsite() {
    let index = build(without_vendor());

    let hits = index
        .lookup("ApiError#bad_request", 50)
        .expect("symbol is indexed");
    assert_eq!(hits.pretty, "ApiError::bad_request");
    assert_eq!(hits.kind, SymKind::Function);
    assert_eq!(hits.definitions.len(), 1);
    assert_eq!(hits.definitions[0].path, "src/error.rs");
    assert_eq!(hits.definitions[0].line, 6);

    assert_eq!(
        hits.counts.references, 3,
        "three callsites in src/server.rs"
    );
    assert!(
        hits.references
            .iter()
            .all(|site| site.path == "src/server.rs")
    );
    assert!(
        hits.references
            .iter()
            .all(|site| site.kind == OccKind::Call)
    );
    let contexts: Vec<&str> = hits
        .references
        .iter()
        .filter_map(|site| site.context)
        .collect();
    assert_eq!(contexts, vec!["get_blob", "get_blob", "get_tree"]);
}

#[test]
fn bare_symbol_collects_every_owner() {
    let index = build(IndexOptions::default());

    let bare = index
        .lookup("#bad_request", 50)
        .expect("bare symbol is indexed");
    assert_eq!(bare.pretty, "bad_request");
    assert_eq!(
        bare.definitions.len(),
        2,
        "vendor/copy.rs is indexed when not excluded: {:?}",
        bare.definitions
    );
    assert_eq!(bare.counts.references, 3);
}

#[test]
fn excluded_paths_are_skipped() {
    let index = build(without_vendor());

    let bare = index.lookup("#bad_request", 50).unwrap();
    assert_eq!(bare.definitions.len(), 1);
    assert_eq!(
        index.stats().files,
        2,
        "only the two src files were indexed"
    );
    assert_eq!(
        index.stats().skipped,
        2,
        "vendor/copy.rs is excluded and README.md is not a tagged language"
    );
}

#[test]
fn search_matches_names_and_owners() {
    let index = build(without_vendor());

    let hits = search(&index, "bad_req");
    assert!(hits.iter().any(|sym| sym == "ApiError#bad_request"));
    assert!(hits.iter().any(|sym| sym == "#bad_request"), "got {hits:?}");

    assert_eq!(
        search(&index, "ApiError::bad"),
        vec!["ApiError#bad_request"],
        "owner narrows the search"
    );

    assert!(search(&index, "get_").iter().any(|sym| sym == "#get_blob"));
    assert!(search(&index, "nothing_here").is_empty());
}

#[test]
fn search_matches_inside_a_name() {
    let mut files = fixture();
    files.push(source("src/frame.rs", FRAME_RS));
    let index = SymbolIndex::build(files, &without_vendor());

    let hits = search(&index, "send");
    assert!(
        hits.iter().any(|sym| sym == "#try_send_frame"),
        "a prefix search would miss it: {hits:?}"
    );
    assert_eq!(
        hits.first().map(String::as_str),
        Some("#send_frame"),
        "a name that starts with the query ranks first: {hits:?}"
    );

    let owner = search(&index, "apierror");
    assert!(
        owner.iter().any(|sym| sym == "ApiError#bad_request"),
        "the owner matches too: {owner:?}"
    );
}

#[test]
fn listing_a_kind_collects_every_symbol_of_it() {
    let index = build(without_vendor());

    let classes = index.list(&[SymKind::Class], Scope::default(), None);
    assert_eq!(
        syms(&classes),
        vec!["#ApiError"],
        "one struct, and no functions or fields"
    );

    let functions = index.list(&[SymKind::Function], Scope::default(), None);
    let functions = syms(&functions);
    assert!(functions.contains(&"#get_blob"), "got {functions:?}");
    assert!(
        functions.contains(&"ApiError#bad_request"),
        "got {functions:?}"
    );
    assert!(!functions.contains(&"#ApiError"), "got {functions:?}");

    let filtered = index.list(&[SymKind::Function], Scope::default(), Some("get_t"));
    assert_eq!(syms(&filtered), vec!["#get_tree"], "a query narrows a kind");

    assert!(
        index
            .list(&[SymKind::Interface], Scope::default(), None)
            .is_empty(),
        "the fixture has no traits"
    );
}

#[test]
fn documentation_mentions_are_not_indexed() {
    let index = build(IndexOptions::default());

    for hit in index.lookup("#bad_request", 50).unwrap().references {
        assert_ne!(hit.path, "README.md", "markdown is not a tagged language");
    }
}

const WINDOW_H: &str = r#"class Window {
 public:
  Window();
};
"#;

const WINDOW_CPP: &str = r#"Window::Window() {}
"#;

#[test]
fn the_most_telling_definition_names_the_kind() {
    let index = SymbolIndex::build(
        vec![
            source("src/dom/Window.cpp", WINDOW_CPP),
            source("src/dom/Window.h", WINDOW_H),
        ],
        &IndexOptions::default(),
    );

    let hits = index.lookup("#Window", 50).expect("symbol is indexed");
    assert_eq!(
        hits.kind,
        SymKind::Class,
        "the constructor is parsed first, but the class says more"
    );
}

const TRACKER_MJS: &str = r#"export const WindowTracker = {
  getTopWindow() {
    return this._windows[0];
  },
};
"#;

const TRACKER_USER_MJS: &str = r#"export function focus() {
  app.WindowTracker.getTopWindow().focus();
}
"#;

const TRACKER_TEST_JS: &str = r#"const WindowTracker = Modules.import("x").WindowTracker;

WindowTracker.getTopWindow();
"#;

fn tracker_index() -> SymbolIndex {
    SymbolIndex::build(
        vec![
            source("src/test/general/test_tracker.js", TRACKER_TEST_JS),
            source("src/modules/WindowTracker.mjs", TRACKER_MJS),
            source("src/modules/Startup.mjs", TRACKER_USER_MJS),
        ],
        &IndexOptions::default(),
    )
}

#[test]
fn product_code_outranks_test_code() {
    let index = tracker_index();

    let hits = index
        .lookup("#WindowTracker", 1)
        .expect("symbol is indexed");
    assert_eq!(
        hits.definitions[0].path, "src/modules/WindowTracker.mjs",
        "the module beats the test that imports it, even at limit 1"
    );
    assert_eq!(
        hits.counts.definitions, 2,
        "the cap does not change the count"
    );
    assert!(hits.truncated);

    let refs: Vec<&str> = hits.references.iter().map(|site| site.path).collect();
    assert_eq!(
        refs.first(),
        Some(&"src/modules/Startup.mjs"),
        "test references sort last: {refs:?}"
    );
}

#[test]
fn object_literal_methods_are_owned_by_their_binding() {
    let index = tracker_index();

    let hits = index
        .lookup("WindowTracker#getTopWindow", 50)
        .expect("symbol is indexed");
    assert_eq!(hits.definitions.len(), 1);
    assert_eq!(hits.definitions[0].path, "src/modules/WindowTracker.mjs");
    assert_eq!(hits.counts.references, 2);
}

/// A query that ignores case reads the whole symbol table, where the bare
/// record of a name already holds every occurrence of it, owned ones
/// included: an unowned query must not count those a second time.
#[test]
fn ignoring_case_counts_each_occurrence_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.tags");
    build(IndexOptions::default()).store_to(&path).unwrap();
    let index = SymbolIndex::mapped_at(&path).expect("the index maps");

    let cased = index.lookup("bad_request", 50).expect("symbol is indexed");
    let any = index
        .lookup_any_case("BAD_REQUEST", 50)
        .expect("any casing of it is indexed too");
    assert_eq!(
        (any.counts.definitions, any.counts.references),
        (cased.counts.definitions, cased.counts.references)
    );
    assert_eq!(
        any.definitions.len() + any.references.len(),
        cased.definitions.len() + cased.references.len(),
        "and it lists each site once"
    );

    let owned = index
        .lookup_any_case("apierror::BAD_REQUEST", 50)
        .expect("the owner matches without regard to case");
    assert_eq!(owned.counts.definitions, cased.counts.definitions);
}
