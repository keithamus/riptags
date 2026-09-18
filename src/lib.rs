//! Fast symbol search over a source tree: definitions and cross-references
//! extracted with tree-sitter.
//!
//! Four layers, cheapest first:
//!
//! * [`file_spans`] tags a single file and needs nothing but its source, so a
//!   viewer can make identifiers clickable immediately.
//! * [`SymbolIndex`] cross-references a set of files: "where is this defined"
//!   and "find all callsites".
//! * [`Cache`] answers from the index on disk, either as it stands or after
//!   checking the tree, and writes back what it had to parse; it also brings
//!   that index up to the tree in the background.
//! * [`watch::run`] keeps the index current while a tree is edited.
//!
//! An index of something immutable, such as a commit, belongs to whoever
//! built it: [`SymbolIndex::store_to`] and [`SymbolIndex::mapped_at`] keep
//! one at a path the caller chooses.
//!
//! A symbol has a bare form (`#name`) that collects every same-named thing in
//! the tree, and an owner-qualified form (`ApiError#bad_request`) that narrows
//! it to one type. Each occurrence records both, so a click can offer either.

pub mod cache;
pub mod index;
pub mod lang;
pub mod symbol;
pub mod tagfile;
pub mod tags;
pub mod walk;
pub mod watch;

pub use cache::{Cache, Freshness};
pub use index::{
    FileTags, IndexOptions, IndexStats, KindCount, LangCount, Overview, Scope, SearchHit,
    SourceFile, SymbolCounts, SymbolHits, SymbolIndex, SymbolSite,
};
pub use lang::{Lang, LangId};
pub use symbol::{OWNER_SEP, OccKind, Span, SymKind, Symbol, Tag};
pub use tags::{file_spans, tag_source};
