use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::lang::LangId;
use crate::symbol::{OccKind, SymKind, Symbol, Tag};
use crate::tagfile::{FileView, SymView, TagFile, TagFileWriter, TagView};
use crate::tags::tag_source;
use crate::walk::{FileMeta, Walk};

/// One file to index.
///
/// `path` is relative to the indexed root and uses `/` separators. `mtime_ns`
/// and `size` feed change detection; they are 0 when the source is not a file
/// on disk.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub mtime_ns: i64,
    pub size: u64,
    pub bytes: Vec<u8>,
}

impl SourceFile {
    /// Tag the file, or `None` when it is not one an index covers.
    ///
    /// A file whose bytes are not UTF-8 is covered and simply has no tags: it
    /// still gets a record, so change detection sees it as up to date instead
    /// of reparsing it on every run.
    pub(crate) fn tag(self, opts: &IndexOptions) -> Option<FileTags> {
        if !opts.covers(&self.path, self.bytes.len() as u64) {
            return None;
        }
        let tags = match std::str::from_utf8(&self.bytes) {
            Ok(source) => tag_source(&self.path, source),
            Err(_) => Vec::new(),
        };
        Some(FileTags {
            path: self.path,
            mtime_ns: self.mtime_ns,
            size: self.size,
            tags,
        })
    }
}

/// The tags of one freshly parsed file, with the stamp that says whether they
/// are stale.
#[derive(Clone, Debug)]
pub struct FileTags {
    pub path: String,
    pub mtime_ns: i64,
    pub size: u64,
    pub tags: Vec<Tag>,
}

/// Limits applied while indexing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IndexOptions {
    /// Files larger than this are skipped.
    pub max_file_bytes: usize,
    /// Path prefixes and path segments to skip (e.g. `vendor`, `third_party/`).
    pub exclude: Vec<String>,
    /// Index the files and directories whose name starts with a dot.
    pub hidden: bool,
    /// Index the files that `.gitignore` and `.ignore` keep out.
    pub no_ignore: bool,
}

impl Default for IndexOptions {
    fn default() -> Self {
        IndexOptions {
            max_file_bytes: 1 << 20,
            exclude: Vec::new(),
            hidden: false,
            no_ignore: false,
        }
    }
}

impl IndexOptions {
    /// Whether a file at `path` with `size` bytes is indexed.
    pub fn covers(&self, path: &str, size: u64) -> bool {
        size <= self.max_file_bytes as u64
            && LangId::of_path(path).is_some()
            && !self.excludes(path)
    }

    pub fn excludes(&self, path: &str) -> bool {
        self.exclude.iter().any(|pattern| {
            let pattern = pattern.trim_end_matches('/');
            path.strip_prefix(pattern)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                || path.split('/').any(|segment| segment == pattern)
        })
    }
}

/// Which files an answer counts: the languages it keeps, and a filter on
/// the paths it keeps.
///
/// An empty scope admits every file, which is what a query with no filter
/// asks for.
#[derive(Clone, Copy, Default)]
pub struct Scope<'a> {
    langs: &'a [LangId],
    paths: Option<&'a dyn Fn(&str) -> bool>,
}

impl<'a> Scope<'a> {
    /// The scope of the languages named, or every language when none are.
    pub fn of(langs: &'a [LangId]) -> Scope<'a> {
        Scope { langs, paths: None }
    }

    /// The same scope, narrowed to the paths `keep` admits.
    pub fn paths(self, keep: &'a dyn Fn(&str) -> bool) -> Scope<'a> {
        Scope {
            paths: Some(keep),
            ..self
        }
    }

    /// Whether a file at `path` is counted.
    pub fn admits(&self, path: &str) -> bool {
        LangId::admits(self.langs, path) && self.paths.is_none_or(|keep| keep(path))
    }

    /// Whether the scope leaves every file in.
    fn everything(&self) -> bool {
        self.langs.is_empty() && self.paths.is_none()
    }
}

/// What indexing produced.
#[derive(Clone, Debug, Default, Serialize)]
pub struct IndexStats {
    pub files: usize,
    pub skipped: usize,
    /// Files whose tags came from the index on disk instead of a fresh parse.
    pub reused: usize,
    pub occurrences: usize,
    pub duration_ms: u64,
}

/// One place a symbol occurs.
#[derive(Clone, Debug, Serialize)]
pub struct SymbolSite<'a> {
    pub path: &'a str,
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub kind: OccKind,
    /// Enclosing definition, for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<&'a str>,
}

impl<'a> SymbolSite<'a> {
    fn of(path: &'a str, tag: &TagView<'a>) -> Self {
        SymbolSite {
            path,
            line: tag.line,
            col: tag.col,
            len: tag.len,
            kind: tag.kind,
            context: tag.context,
        }
    }

    /// Sort key: product code before test code, then the file named after
    /// the symbol, then by path and line.
    fn rank(&self, name: &str, owner: Option<&str>) -> (bool, bool, &'a str, u32) {
        (
            self.in_tests(),
            !self.names_file(name, owner),
            self.path,
            self.line,
        )
    }

    /// Does the site hold test code? A fixture that names a symbol ranks
    /// after the product code that defines it.
    fn in_tests(&self) -> bool {
        self.path
            .split('/')
            .any(|segment| contains_ci(segment, "test"))
    }

    /// Is the site in the file named after the symbol? `ApiError.ts` answers
    /// for both `ApiError` and `ApiError#bad_request`.
    fn names_file(&self, name: &str, owner: Option<&str>) -> bool {
        let file = self.path.rsplit('/').next().unwrap_or(self.path);
        let stem = file.split('.').next().unwrap_or(file);
        stem.eq_ignore_ascii_case(name)
            || owner.is_some_and(|owner| stem.eq_ignore_ascii_case(owner))
    }
}

/// How many occurrences of each group a symbol has, before any cap.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SymbolCounts {
    pub definitions: usize,
    pub declarations: usize,
    pub references: usize,
    pub implementations: usize,
}

impl SymbolCounts {
    pub fn total(&self) -> usize {
        self.definitions + self.declarations + self.references + self.implementations
    }
}

/// Every occurrence of one symbol, grouped the way the UI shows them.
#[derive(Clone, Debug, Serialize)]
pub struct SymbolHits<'a> {
    pub sym: String,
    pub pretty: String,
    pub kind: SymKind,
    pub definitions: Vec<SymbolSite<'a>>,
    pub declarations: Vec<SymbolSite<'a>>,
    pub references: Vec<SymbolSite<'a>>,
    pub implementations: Vec<SymbolSite<'a>>,
    pub counts: SymbolCounts,
    /// True when a group was capped by `limit`.
    pub truncated: bool,
}

impl<'a> SymbolHits<'a> {
    fn new(sym: Symbol<'_>) -> Self {
        SymbolHits {
            pretty: sym.pretty(),
            sym: sym.stored(),
            kind: SymKind::Unknown,
            definitions: Vec::new(),
            declarations: Vec::new(),
            references: Vec::new(),
            implementations: Vec::new(),
            counts: SymbolCounts::default(),
            truncated: false,
        }
    }

    /// Each group beside its uncapped count, in display order.
    pub fn groups(&mut self) -> [(&mut Vec<SymbolSite<'a>>, &mut usize); 4] {
        [
            (&mut self.definitions, &mut self.counts.definitions),
            (&mut self.declarations, &mut self.counts.declarations),
            (&mut self.implementations, &mut self.counts.implementations),
            (&mut self.references, &mut self.counts.references),
        ]
    }

    /// The site that stands for the symbol: its definition, or wherever it
    /// occurs when nothing in the tree defines it.
    pub fn best(&self) -> Option<&SymbolSite<'a>> {
        [
            &self.definitions,
            &self.declarations,
            &self.implementations,
            &self.references,
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// Keep only the sites `keep` admits, then cap each group at `limit`.
    ///
    /// The counts follow the sites that survive, so a filtered answer says
    /// how much of it there is rather than how much there was.
    pub fn filter(&mut self, keep: impl Fn(&SymbolSite<'a>) -> bool, limit: usize) {
        let mut truncated = false;
        for (sites, count) in self.groups() {
            sites.retain(&keep);
            *count = sites.len();
            if sites.len() > limit {
                sites.truncate(limit);
                truncated = true;
            }
        }
        self.truncated = truncated;
    }

    fn add(&mut self, path: &'a str, tag: &TagView<'a>) {
        if tag.kind.is_definition() && tag.sym_kind.group_rank() < self.kind.group_rank() {
            self.kind = tag.sym_kind;
        }
        let (sites, count) = match tag.kind {
            OccKind::Definition => (&mut self.definitions, &mut self.counts.definitions),
            OccKind::Declaration => (&mut self.declarations, &mut self.counts.declarations),
            OccKind::Impl => (&mut self.implementations, &mut self.counts.implementations),
            _ => (&mut self.references, &mut self.counts.references),
        };
        *count += 1;
        sites.push(SymbolSite::of(path, tag));
    }
}

/// A symbol matching a search.
#[derive(Clone, Debug, Serialize)]
pub struct SearchHit {
    pub sym: String,
    pub pretty: String,
    pub kind: SymKind,
    pub definitions: usize,
    pub references: usize,
}

impl SearchHit {
    fn new(owner: &str, name: &str, kind: SymKind, definitions: usize, references: usize) -> Self {
        let sym = Symbol::owned(owner, name);
        SearchHit {
            pretty: sym.pretty(),
            sym: sym.stored(),
            kind,
            definitions,
            references,
        }
    }
}

/// What a tree holds, for a caller who has not named a symbol yet.
#[derive(Clone, Debug, Serialize)]
pub struct Overview {
    pub files: usize,
    pub symbols: usize,
    pub occurrences: usize,
    /// Indexed files per language, most files first.
    pub languages: Vec<LangCount>,
    /// Distinct symbols per kind, most symbols first.
    pub kinds: Vec<KindCount>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct LangCount {
    pub language: &'static str,
    pub files: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct KindCount {
    pub kind: SymKind,
    pub symbols: usize,
}

/// Symbol cross-reference over a set of files.
///
/// Mapped files answer through the symbol table of the index on disk: one
/// binary search, then a walk of that symbol's postings. Files reparsed in
/// this process have no table, so their tags are scanned; there are only ever
/// as many of those as the working tree has uncommitted edits.
pub struct SymbolIndex {
    mapped: Option<TagFile>,
    /// Which mapped files are still current; empty when nothing is mapped.
    live: Vec<bool>,
    /// Tag slots of the mapped files that changed, in slot order.
    stale: Vec<(u32, u32)>,
    fresh: Vec<FileTags>,
    /// Distinct symbols, counted on the first caller that asks.
    symbols: OnceLock<usize>,
    stats: IndexStats,
}

impl SymbolIndex {
    /// Tag every indexable file, and report how many were skipped.
    ///
    /// Files are pulled as workers free up, so a lazy iterator holds only the
    /// bytes being tagged rather than the contents of the whole tree.
    pub fn tag_files<I>(files: I, opts: &IndexOptions) -> (Vec<FileTags>, usize)
    where
        I: IntoIterator<Item = SourceFile>,
        I::IntoIter: Send,
    {
        let skipped = AtomicUsize::new(0);
        let tag = |file: SourceFile| {
            let tags = file.tag(opts);
            if tags.is_none() {
                skipped.fetch_add(1, AtomicOrdering::Relaxed);
            }
            tags
        };
        #[cfg(feature = "rayon")]
        let tagged: Vec<FileTags> = {
            use rayon::prelude::*;

            files.into_iter().par_bridge().filter_map(tag).collect()
        };
        #[cfg(not(feature = "rayon"))]
        let tagged: Vec<FileTags> = files.into_iter().filter_map(tag).collect();
        (tagged, skipped.into_inner())
    }

    /// Tag and cross-reference every file.
    pub fn build<I>(files: I, opts: &IndexOptions) -> SymbolIndex
    where
        I: IntoIterator<Item = SourceFile>,
        I::IntoIter: Send,
    {
        let started = Instant::now();
        let (tagged, skipped) = Self::tag_files(files, opts);
        Self::from_tags(tagged, skipped, started.elapsed())
    }

    /// Index every supported file under `root`, reading the working tree.
    ///
    /// Each file is read as it is tagged, so the tree costs its paths in
    /// memory rather than its contents.
    pub fn of_dir(root: &Path, opts: &IndexOptions) -> Result<SymbolIndex> {
        let started = Instant::now();
        let files = Walk::new(root, opts).meta()?;
        let (tagged, skipped) = Self::tag_files(files.into_iter().filter_map(FileMeta::read), opts);
        Ok(Self::from_tags(tagged, skipped, started.elapsed()))
    }

    /// An index over freshly parsed tags alone.
    ///
    /// `took` is what assembling them cost, which a caller that tags in
    /// stages accumulates itself.
    pub fn from_tags(mut tagged: Vec<FileTags>, skipped: usize, took: Duration) -> SymbolIndex {
        tagged.sort_by(|a, b| a.path.cmp(&b.path));
        let stats = IndexStats {
            files: tagged.len(),
            skipped,
            reused: 0,
            occurrences: tagged.iter().map(|file| file.tags.len()).sum(),
            duration_ms: took.as_millis() as u64,
        };
        SymbolIndex {
            mapped: None,
            live: Vec::new(),
            stale: Vec::new(),
            fresh: tagged,
            symbols: OnceLock::new(),
            stats,
        }
    }

    /// An index over the file on disk, with `live` marking the mapped files
    /// that are still current and `fresh` holding the reparsed ones.
    pub fn from_mapped(
        mapped: TagFile,
        live: Vec<bool>,
        mut fresh: Vec<FileTags>,
        skipped: usize,
        reused: usize,
        took: Duration,
    ) -> SymbolIndex {
        fresh.sort_by(|a, b| a.path.cmp(&b.path));
        let mut occurrences: usize = fresh.iter().map(|file| file.tags.len()).sum();
        let mut stale = Vec::new();
        for (file, kept) in mapped.files().zip(&live) {
            if *kept {
                occurrences += file.tag_count();
            } else if file.tag_count() > 0 {
                stale.push((file.tag_start(), file.tag_start() + file.tag_count() as u32));
            }
        }
        let stats = IndexStats {
            files: reused + fresh.len(),
            skipped,
            reused,
            occurrences,
            duration_ms: took.as_millis() as u64,
        };
        SymbolIndex {
            mapped: Some(mapped),
            live,
            stale,
            fresh,
            symbols: OnceLock::new(),
            stats,
        }
    }

    /// An index over the file on disk alone, taken to match the tree.
    ///
    /// Nothing is walked and nothing is parsed, so a query costs what the
    /// answer costs. Edits made since the index was written are invisible.
    pub fn of_index(mapped: TagFile, took: Duration) -> SymbolIndex {
        let live = vec![true; mapped.file_count()];
        let reused = live.len();
        Self::from_mapped(mapped, live, Vec::new(), 0, reused, took)
    }

    /// The index written at `path`, taken as it stands.
    ///
    /// Pair it with [`SymbolIndex::store_to`] to keep an index somewhere of
    /// the caller's choosing, such as one file per immutable snapshot.
    pub fn mapped_at(path: &Path) -> Option<SymbolIndex> {
        Some(Self::of_index(TagFile::open(path)?, Duration::ZERO))
    }

    /// Write the index to `path`: mapped files that are still current keep
    /// their records, changed files contribute new ones.
    ///
    /// Both runs arrive in path order and are merged in that order, because
    /// the reader binary-searches the file records by path.
    ///
    /// What the file at `path` describes is the caller's business. The cache
    /// keys it by the directory it indexes and stamps each file with its
    /// mtime and size; an index of something immutable, such as a commit, can
    /// key it by that instead and leave the stamps empty.
    pub fn store_to(&self, path: &Path) -> Result<()> {
        let mut writer = TagFileWriter::default();
        let mut mapped = self.mapped_files().peekable();
        let mut fresh = self.fresh.iter().peekable();
        loop {
            let take_fresh = match (mapped.peek(), fresh.peek()) {
                (Some(kept), Some(new)) => new.path.as_str() < kept.path(),
                (None, Some(_)) => true,
                (Some(_), None) => false,
                (None, None) => break,
            };
            if take_fresh {
                let file = fresh.next().expect("peeked");
                writer.file(&file.path, file.mtime_ns, file.size);
                for tag in &file.tags {
                    writer.push(TagView::of(tag));
                }
            } else {
                let file = mapped.next().expect("peeked");
                writer.file(file.path(), file.mtime_ns(), file.size());
                for tag in file.tags() {
                    writer.push(tag);
                }
            }
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        // Rename, never write in place: readers hold a memory map of the old
        // file and must keep seeing intact bytes.
        let stored =
            std::fs::write(&tmp, writer.finish()).and_then(|()| std::fs::rename(&tmp, path));
        if stored.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        stored.with_context(|| format!("replace {}", path.display()))
    }

    pub fn stats(&self) -> &IndexStats {
        &self.stats
    }

    /// Whether the index on disk is behind the tags this index holds: some
    /// file was reparsed, dropped, or never written.
    pub fn drifted(&self) -> bool {
        self.mapped.is_none() || !self.fresh.is_empty() || self.live.iter().any(|kept| !kept)
    }

    /// The mapped files that are still current.
    pub fn mapped_files(&self) -> impl Iterator<Item = FileView<'_>> + '_ {
        self.mapped
            .iter()
            .flat_map(|mapped| mapped.files().zip(&self.live))
            .filter_map(|(file, kept)| kept.then_some(file))
    }

    /// The files parsed in this process.
    pub fn fresh_files(&self) -> &[FileTags] {
        &self.fresh
    }

    /// Whether the mapped tag in `slot` still reflects the file on disk.
    fn current(&self, slot: u32) -> bool {
        !in_ranges(&self.stale, slot)
    }

    /// Tag slots of the mapped files `scope` leaves out, in slot order.
    /// Empty when every file is wanted.
    ///
    /// A file holds a run of consecutive slots, so a whole language costs a
    /// range per file and a binary search per slot, rather than a path
    /// lookup per occurrence.
    fn excluded_slots(&self, scope: Scope<'_>) -> Vec<(u32, u32)> {
        if scope.everything() {
            return Vec::new();
        }
        self.mapped
            .iter()
            .flat_map(TagFile::files)
            .filter(|file| file.tag_count() > 0 && !scope.admits(file.path()))
            .map(|file| {
                let start = file.tag_start();
                (start, start + file.tag_count() as u32)
            })
            .collect()
    }

    /// The mapped tag in `slot` with the path of its file, when the file is
    /// still current.
    fn mapped_tag<'a>(&self, map: &'a TagFile, slot: u32) -> Option<(&'a str, TagView<'a>)> {
        if !self.current(slot) {
            return None;
        }
        let path = map.file_of(slot).map_or("", |file| file.path());
        map.tag(slot).map(|tag| (path, tag))
    }

    /// The mapped symbol `sym` names, when the index on disk holds it.
    fn mapped_symbol(&self, owner: Option<&str>, name: &str) -> Option<(&TagFile, SymView<'_>)> {
        let map = self.mapped.as_ref()?;
        let sym = map.symbol(owner.unwrap_or(""), name)?;
        Some((map, sym))
    }

    /// Does the index on disk still hold `owner#name` in a current file?
    fn mapped_holds(&self, owner: &str, name: &str) -> bool {
        self.mapped_symbol(Some(owner), name)
            .is_some_and(|(_, sym)| sym.slots().any(|(slot, _)| self.current(slot)))
    }

    /// How many distinct symbols the tags name.
    ///
    /// The pass over the tags is made once: an index that reports its symbol
    /// count more than once pays for it on the first call alone.
    pub fn symbol_count(&self) -> usize {
        *self.symbols.get_or_init(|| self.count_symbols())
    }

    fn count_symbols(&self) -> usize {
        let mapped = match &self.mapped {
            Some(map) if self.stale.is_empty() => map.symbol_count(),
            Some(map) => map
                .symbols()
                .filter(|sym| sym.slots().any(|(slot, _)| self.current(slot)))
                .count(),
            None => 0,
        };
        let mut fresh: HashSet<(&str, &str)> = HashSet::new();
        for (_, tag) in self.fresh_tags() {
            for owner in tag.owners().chain(std::iter::once("")) {
                if !self.mapped_holds(owner, tag.name) {
                    fresh.insert((owner, tag.name));
                }
            }
        }
        mapped + fresh.len()
    }

    /// Every tag parsed in this process, with the path of its file.
    fn fresh_tags(&self) -> impl Iterator<Item = (&str, TagView<'_>)> + '_ {
        self.fresh
            .iter()
            .flat_map(|file| file.tags.iter().map(|tag| (&*file.path, TagView::of(tag))))
    }

    /// Every occurrence of `sym`, grouped by kind and capped at `limit` per
    /// group. `sym` may be typed (`Owner::name`, `name`) or stored
    /// (`Owner#name`, `#name`).
    ///
    /// Each group is ordered before the cap applies: product code before test
    /// code, then the file named after the symbol, then by path. Reparsed
    /// files hold the edits the caller just made, so they join the same order
    /// rather than jumping it.
    pub fn lookup(&self, sym: &str, limit: usize) -> Option<SymbolHits<'_>> {
        self.lookup_with(sym, limit, true)
    }

    /// The same as [`SymbolIndex::lookup`], with the name and the owner
    /// matched without regard to case.
    ///
    /// The index is ordered by name, so a search that ignores case reads
    /// every symbol rather than jumping to one: it costs a pass over the
    /// symbol table.
    pub fn lookup_any_case(&self, sym: &str, limit: usize) -> Option<SymbolHits<'_>> {
        self.lookup_with(sym, limit, false)
    }

    fn lookup_with(&self, sym: &str, limit: usize, cased: bool) -> Option<SymbolHits<'_>> {
        let wanted = Symbol::parse(sym);
        let (owner, name) = (wanted.owner, wanted.name);
        let same = |held: &str, asked: &str| match cased {
            true => held == asked,
            false => held.eq_ignore_ascii_case(asked),
        };
        let mut hits = SymbolHits::new(wanted);
        for (path, tag) in self.fresh_tags() {
            let owned = owner.is_none_or(|owner| match cased {
                true => tag.owned_by(owner),
                false => tag.owners().any(|held| same(held, owner)),
            });
            if same(tag.name, name) && owned {
                hits.add(path, &tag);
            }
        }
        if cased {
            if let Some((map, sym)) = self.mapped_symbol(owner, name) {
                for (slot, _) in sym.slots() {
                    if let Some((path, tag)) = self.mapped_tag(map, slot) {
                        hits.add(path, &tag);
                    }
                }
            }
        } else if let Some(map) = self.mapped.as_ref() {
            // The bare record already holds a posting for every occurrence of
            // the name, owned ones included, so an unowned query walks that
            // record alone or it counts them twice.
            let matching = map.symbols().filter(|sym| {
                same(sym.name(), name)
                    && match owner {
                        Some(owner) => same(sym.owner(), owner),
                        None => sym.owner().is_empty(),
                    }
            });
            for sym in matching {
                for (slot, _) in sym.slots() {
                    if let Some((path, tag)) = self.mapped_tag(map, slot) {
                        hits.add(path, &tag);
                    }
                }
            }
        }
        if hits.counts.total() == 0 {
            return None;
        }
        let mut truncated = false;
        for (sites, _) in hits.groups() {
            sites.sort_by_cached_key(|site| site.rank(name, owner));
            if sites.len() > limit {
                sites.truncate(limit);
                truncated = true;
            }
        }
        hits.truncated = truncated;
        Some(hits)
    }

    /// Visit every symbol the filters admit, each folded from the index on
    /// disk and the tags parsed in this process. A symbol counts only the
    /// occurrences that sit in a file `scope` admits.
    fn fold_symbols<'a>(
        &'a self,
        filter: &NameQuery,
        scope: Scope<'_>,
        mut visit: impl FnMut(Folded<'a>),
    ) {
        let excluded = self.excluded_slots(scope);
        // Fresh tags first: (rank, kind, definitions, references) per symbol,
        // folded into the mapped symbol of the same name when there is one.
        let mut fresh: HashMap<(&str, &str), (u8, SymKind, usize, usize)> = HashMap::new();
        for (path, tag) in self.fresh_tags() {
            if !scope.admits(path) {
                continue;
            }
            for owner in tag.owners().chain(std::iter::once("")) {
                let Some(rank) = filter.rank(owner, tag.name) else {
                    continue;
                };
                let entry =
                    fresh
                        .entry((owner, tag.name))
                        .or_insert((rank, SymKind::Unknown, 0, 0));
                if tag.kind.is_definition() {
                    entry.2 += 1;
                    if tag.sym_kind.group_rank() < entry.1.group_rank() {
                        entry.1 = tag.sym_kind;
                    }
                } else {
                    entry.3 += 1;
                }
            }
        }

        for sym in self.mapped.iter().flat_map(TagFile::symbols) {
            let (owner, name) = (sym.owner(), sym.name());
            let Some(rank) = filter.rank(owner, name) else {
                continue;
            };
            let (mut definitions, mut occurrences) = (sym.definitions(), sym.occurrences());
            if !self.stale.is_empty() || !excluded.is_empty() {
                let current = sym
                    .slots()
                    .filter(|(slot, _)| self.current(*slot) && !in_ranges(&excluded, *slot));
                (definitions, occurrences) = current.fold((0, 0), |(defs, all), (_, kind)| {
                    (defs + usize::from(kind.is_definition()), all + 1)
                });
            }
            let mut references = occurrences - definitions;
            let mut kind = sym.kind();
            if let Some((_, fresh_kind, fresh_defs, fresh_refs)) = fresh.remove(&(owner, name)) {
                definitions += fresh_defs;
                references += fresh_refs;
                if fresh_kind.group_rank() < kind.group_rank() {
                    kind = fresh_kind;
                }
            }
            if definitions + references > 0 {
                visit(Folded {
                    rank,
                    owner,
                    name,
                    kind,
                    definitions,
                    references,
                });
            }
        }
        for ((owner, name), (rank, kind, definitions, references)) in fresh {
            visit(Folded {
                rank,
                owner,
                name,
                kind,
                definitions,
                references,
            });
        }
    }

    /// Symbols of the given kinds inside `scope`, optionally narrowed by
    /// `query`.
    ///
    /// Empty `kinds` means every kind, an empty scope every file, and a
    /// `None` query every symbol. A query may be owner-qualified
    /// (`ApiError::bad`, `ApiError#bad`), which narrows the match to owners
    /// containing that part. Name matches rank before owner-only matches,
    /// and prefix matches before infix ones.
    pub fn list(&self, kinds: &[SymKind], scope: Scope<'_>, query: Option<&str>) -> Vec<SearchHit> {
        let query = query.map(str::trim).filter(|query| !query.is_empty());
        let filter = NameQuery::parse(query);
        let mut hits: Vec<(u8, SearchHit)> = Vec::new();
        self.fold_symbols(&filter, scope, |sym| {
            if kinds.is_empty() || kinds.contains(&sym.kind) {
                hits.push((
                    sym.rank,
                    SearchHit::new(
                        sym.owner,
                        sym.name,
                        sym.kind,
                        sym.definitions,
                        sym.references,
                    ),
                ));
            }
        });
        if query.is_none() {
            hits.sort_by(|(_, a), (_, b)| a.pretty.cmp(&b.pretty));
        } else {
            hits.sort_by(|(a_rank, a), (b_rank, b)| {
                a_rank
                    .cmp(b_rank)
                    .then_with(|| b.definitions.cmp(&a.definitions))
                    .then_with(|| a.sym.len().cmp(&b.sym.len()))
                    .then_with(|| a.sym.cmp(&b.sym))
            });
        }
        hits.into_iter().map(|(_, hit)| hit).collect()
    }

    /// What the tree holds: how much of it there is, in which languages, and
    /// how its symbols divide by kind. A narrowed scope describes that part
    /// of the tree alone.
    ///
    /// One pass over the symbol table, allocating nothing per symbol, so the
    /// cost follows the size of the index rather than the size of the answer.
    pub fn overview(&self, scope: Scope<'_>) -> Overview {
        let mut per_lang = vec![0usize; LangId::ALL.len()];
        let (mut files, mut occurrences) = (0, 0);
        let counted = self
            .mapped_files()
            .map(|file| (file.path(), file.tag_count()))
            .chain(
                self.fresh
                    .iter()
                    .map(|file| (file.path.as_str(), file.tags.len())),
            );
        for (path, tags) in counted {
            let Some(lang) = LangId::of_path(path).filter(|_| scope.admits(path)) else {
                continue;
            };
            per_lang[lang as usize] += 1;
            files += 1;
            occurrences += tags;
        }
        let mut languages: Vec<LangCount> = LangId::ALL
            .iter()
            .zip(&per_lang)
            .filter(|(_, files)| **files > 0)
            .map(|(lang, files)| LangCount {
                language: lang.name(),
                files: *files,
            })
            .collect();
        languages.sort_by(|a, b| {
            b.files
                .cmp(&a.files)
                .then_with(|| a.language.cmp(b.language))
        });

        let mut per_kind = [0usize; SymKind::ALL.len()];
        let mut symbols = 0;
        self.fold_symbols(&NameQuery::parse(None), scope, |sym| {
            per_kind[sym.kind as usize] += 1;
            symbols += 1;
        });
        let mut kinds: Vec<KindCount> = SymKind::ALL
            .iter()
            .zip(&per_kind)
            .filter(|(_, symbols)| **symbols > 0)
            .map(|(kind, symbols)| KindCount {
                kind: *kind,
                symbols: *symbols,
            })
            .collect();
        kinds.sort_by_key(|kind| std::cmp::Reverse(kind.symbols));

        Overview {
            files,
            symbols,
            occurrences,
            languages,
            kinds,
        }
    }

    /// Symbols whose name or owner matches `query`, best match first, capped
    /// at `limit`.
    ///
    /// The cap loses how many there were: a caller that wants to say "10 of
    /// 300" ranks them with [`SymbolIndex::list`] and truncates itself.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let mut hits = self.list(&[], Scope::default(), Some(query));
        hits.truncate(limit);
        hits
    }
}

/// One symbol as the index holds it: the tags on disk and the tags parsed in
/// this process, added together.
struct Folded<'a> {
    /// How well it answers the query, for a listing that ranks its hits.
    rank: u8,
    owner: &'a str,
    name: &'a str,
    kind: SymKind,
    definitions: usize,
    references: usize,
}

/// A name filter: the query a listing was given, matched against one
/// `owner#name` at a time.
struct NameQuery {
    owner: Option<String>,
    name: Option<String>,
}

impl NameQuery {
    /// Parse the typed form (`Owner::name`, `name`) or the stored form,
    /// lowercased once so matching can ignore case. No query matches
    /// everything.
    fn parse(query: Option<&str>) -> NameQuery {
        let Some(query) = query else {
            return NameQuery {
                owner: None,
                name: None,
            };
        };
        let sym = Symbol::parse(query);
        NameQuery {
            owner: sym.owner.map(str::to_ascii_lowercase),
            name: Some(sym.name.to_ascii_lowercase()),
        }
    }

    /// How well `owner#name` answers the query, or `None` when it does not.
    ///
    /// Lower ranks come first: a name prefix, then a name infix, then an
    /// owner-only match.
    fn rank(&self, owner: &str, name: &str) -> Option<u8> {
        let Some(name_query) = self.name.as_deref() else {
            return Some(0);
        };
        match self.owner.as_deref() {
            Some(owner_query) => {
                if !contains_ci(owner, owner_query) || !contains_ci(name, name_query) {
                    return None;
                }
                Some(u8::from(!starts_ci(name, name_query)))
            }
            None => {
                if starts_ci(name, name_query) {
                    Some(0)
                } else if contains_ci(name, name_query) {
                    Some(1)
                } else if !owner.is_empty() && contains_ci(owner, name_query) {
                    Some(2)
                } else {
                    None
                }
            }
        }
    }
}

/// Is `slot` inside one of these sorted, disjoint slot ranges?
fn in_ranges(ranges: &[(u32, u32)], slot: u32) -> bool {
    ranges
        .binary_search_by(|(start, end)| {
            if slot < *start {
                Ordering::Greater
            } else if slot >= *end {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// `needle` must already be ASCII lowercase.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// `needle` must already be ASCII lowercase.
fn starts_ci(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .get(..needle.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(needle.as_bytes()))
}
