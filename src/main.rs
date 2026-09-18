use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter, StdoutLock, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail};
use clap::{ArgAction, ArgGroup, ColorChoice, Parser};
use ignore::overrides::{Override, OverrideBuilder};
use riptags::walk::Walk;
use riptags::{
    Cache, Freshness, IndexOptions, IndexStats, LangId, Scope, SymKind, Symbol, SymbolHits,
    SymbolIndex, SymbolSite, watch,
};
use serde::Serialize;

mod style;
use style::Style;

const AFTER_HELP: &str = "\
EXAMPLES:
  rt                                   What this tree holds: files, languages, kinds
  rt bad_request                       Every definition and callsite of bad_request
  rt ApiError::bad_request             Narrowed to the ApiError owner
  rt bad_request --refs                Callsites only
  rt send --search                     Symbol names containing 'send'
  rt --class                           Every class, struct and enum in the tree
  rt Error --class ../path/to/code     Classes matching 'Error' in another tree
  rt --class -t rust                   Classes in the Rust part of a mixed tree
  rt bad_request -g '!**/tests/**'     Sites outside the test tree
  rt bad_request -C 2                  Two source lines either side of each site
  rt bad_request -i                    Any casing of the name
  rt --index                           Build and persist the index, report what it found
  rt --watch                           Follow edits into the index until interrupted
  rt bad_request --vimgrep             One `path:line:col:text` record per site
  rt bad_request -l | xargs nvim       Just the files, for another tool
  nvim -q <(rt bad_request --vimgrep)  Every site as a quickfix list

A record format (--vimgrep, -l) prints no heading, no count and no prose, and
a miss prints nothing at all: exit 1 says it. -q prints nothing either way.
--limit 0 lifts the per-group cap, and -0 ends each record with NUL for
`xargs -0`.

The walk reads what git would: no hidden files, nothing .gitignore excludes,
nothing over 1M. --hidden, --no-ignore (-u, -uu) and --max-filesize widen it,
and each widened walk keeps an index of its own.

A query answers from the index without checking the tree, which is what makes
it cost milliseconds. A query that finds nothing checks the tree before saying
so, and the index is refreshed in the background afterwards. Use --fresh to
check first, or leave `rt --watch` running to keep the index current.";

/// One-screen help for coding agents, in place of clap's prose.
const AGENT_HELP: &str = "\
rt: symbol search over a source tree (tree-sitter tags)
Languages: rust|ts|tsx|js|py|go|c|cpp|java|cs|kt|scala|swift|rb|php|lua|sh|ex|hs|ml|nix|zig|html|css
rt <NAME|Owner::name> [ROOT] definitions + every callsite, with context and source line
  -d|--def definitions only|-r|--refs callsites only|-m|--limit <N> per group (50)|-m0 uncapped
  -g|--glob <GLOB> keep matching files, `!GLOB` drops them|-i ignore case|-q exit code only
  -C|-A|-B <N> source lines around each site|--search names only|--json|--fresh|--no-cache
  -t|--lang <LANG> keep one language (rust|ts|py|cpp...); repeat or comma-separate
  --vimgrep `path:line:col:text` per site|-l paths only|-0 NUL records
rt [FILTER] [ROOT] --class|--func|--method|--iface|--mod|--macro|--const|--field|--var|--typedef
  lists every symbol of those kinds (combinable); FILTER narrows by name/owner
  long spellings work too: --definitions --function --interface --module --constant --variable
rt --files [ROOT] every file rt would index|rt --type-list the languages it knows
Walk skips hidden and gitignored files and anything over 1M: --hidden|--no-ignore|-u|-uu|
  --max-filesize <N> widen it; a widened walk keeps an index of its own
rt [../another/tree] with no symbol and no kind: files, symbols, languages, kinds
rt --index [ROOT] build + persist the index, report files/symbols/occurrences/ms
rt --watch [ROOT] keep the index current until interrupted (one watcher per tree)
Symbols: `name` any owner|`Owner::name` one type; lowercase receivers have no owner, query bare
Answers come from the index without walking the tree; a miss rechecks it, --fresh always does
Index auto-persists (~/.cache/riptags); refreshed in background, or live under --watch
rt --skill the skill sheet for a harness (e.g. ~/.claude/skills/riptags/SKILL.md)
Ex: rt|rt bad_request|rt ApiError::bad_request --refs|rt send --search|rt --class|rt --watch
";

/// The skill sheet, for a harness that loads one.
const SKILL: &str = include_str!("../SKILL.md");

/// The agent help sheet, when an agent harness asked for `--help`.
fn agent_help() -> Option<&'static str> {
    let asked = std::env::args().any(|arg| arg == "--help" || arg == "-h");
    (Style::agent() && asked).then_some(AGENT_HELP)
}

#[derive(Parser, Debug)]
#[command(
    name = "rt",
    version,
    about = "Fast symbol search: definitions and cross-references across a source tree",
    after_long_help = AFTER_HELP,
    group = ArgGroup::new("records")
        .args(["vimgrep", "matching_files", "files"])
        .conflicts_with_all(["json", "index", "watch"])
)]
struct Args {
    /// Symbol name, `Owner::name`, or `Owner#name` (a name filter when listing kinds)
    query: Option<String>,

    /// Directory to search (default: the working directory)
    root: Option<PathBuf>,

    /// List matching symbol names instead of their occurrences
    #[arg(long)]
    search: bool,

    /// Show definitions only
    #[arg(
        short = 'd',
        long,
        visible_alias = "def",
        alias = "defs",
        conflicts_with = "references"
    )]
    definitions: bool,

    /// Show references only
    #[arg(
        short = 'r',
        long = "refs",
        alias = "ref",
        conflicts_with = "definitions"
    )]
    references: bool,

    /// Match the symbol name and owner without regard to case
    #[arg(short = 'i', long = "ignore-case")]
    ignore_case: bool,

    /// Maximum occurrences per group, or 0 for every one of them
    #[arg(short = 'm', long, default_value = "50")]
    limit: usize,

    /// Keep only the files this glob matches; a leading `!` drops them instead
    #[arg(short = 'g', long = "glob", value_name = "GLOB")]
    globs: Vec<String>,

    /// Keep only symbols written in these languages (`rust`, `ts`, `py`)
    #[arg(
        short = 't',
        long = "type",
        visible_alias = "lang",
        alias = "language",
        value_name = "LANG",
        value_delimiter = ','
    )]
    languages: Vec<String>,

    /// Print source lines either side of each site
    #[arg(short = 'C', long, value_name = "NUM", help_heading = "OUTPUT")]
    context: Option<u32>,

    /// Print source lines after each site
    #[arg(
        short = 'A',
        long = "after-context",
        value_name = "NUM",
        help_heading = "OUTPUT"
    )]
    after_context: Option<u32>,

    /// Print source lines before each site
    #[arg(
        short = 'B',
        long = "before-context",
        value_name = "NUM",
        help_heading = "OUTPUT"
    )]
    before_context: Option<u32>,

    /// Emit JSON
    #[arg(long, help_heading = "OUTPUT")]
    json: bool,

    /// Print nothing: the exit code says whether anything matched
    #[arg(short = 'q', long, help_heading = "OUTPUT")]
    quiet: bool,

    /// Say nothing on stderr about the state of the index
    #[arg(long = "no-messages", help_heading = "OUTPUT")]
    no_messages: bool,

    /// Index the files and directories whose name starts with a dot
    #[arg(long, help_heading = "FILTER")]
    hidden: bool,

    /// Index the files `.gitignore` and `.ignore` keep out
    #[arg(long = "no-ignore", help_heading = "FILTER")]
    no_ignore: bool,

    /// Filter less: `-u` indexes ignored files, `-uu` hidden ones as well
    #[arg(
        short = 'u',
        long = "unrestricted",
        action = ArgAction::Count,
        help_heading = "FILTER"
    )]
    unrestricted: u8,

    /// Skip files larger than this, in bytes or with a `K`/`M`/`G` suffix
    #[arg(long = "max-filesize", value_name = "NUM", help_heading = "FILTER")]
    max_filesize: Option<String>,

    /// Print the path of every file rt would index, and nothing else
    #[arg(long, help_heading = "PIPELINES")]
    files: bool,

    /// Print one `path:line:col:text` record per site, for an editor or a pipe
    #[arg(long, help_heading = "PIPELINES")]
    vimgrep: bool,

    /// Print the path of each matching file, once each
    #[arg(short = 'l', long = "files-with-matches", help_heading = "PIPELINES")]
    matching_files: bool,

    /// End each record with NUL instead of a newline, for `xargs -0`
    #[arg(
        short = '0',
        long = "null",
        requires = "records",
        help_heading = "PIPELINES"
    )]
    null: bool,

    /// List the languages rt can index
    #[arg(long = "type-list")]
    type_list: bool,

    /// Print the skill sheet for a coding agent harness
    #[arg(long)]
    skill: bool,

    /// When to colour the output
    #[arg(long, value_enum, value_name = "WHEN", default_value = "auto")]
    color: ColorChoice,

    /// Build and persist the index instead of querying
    #[arg(long)]
    index: bool,

    /// Keep the index of the tree current until interrupted
    #[arg(long)]
    watch: bool,

    /// Check the tree for edits before answering, instead of trusting the index
    #[arg(long)]
    fresh: bool,

    /// Ignore the persisted index: scan the tree and write nothing
    #[arg(long = "no-cache")]
    no_cache: bool,

    /// List every function
    #[arg(
        long,
        visible_alias = "func",
        alias = "fn",
        help_heading = "SYMBOL KINDS"
    )]
    function: bool,

    /// List every method
    #[arg(long, visible_alias = "meth", help_heading = "SYMBOL KINDS")]
    method: bool,

    /// List every class, struct and enum
    #[arg(long, help_heading = "SYMBOL KINDS")]
    class: bool,

    /// List every interface, trait and protocol
    #[arg(
        long,
        visible_alias = "iface",
        alias = "trait",
        help_heading = "SYMBOL KINDS"
    )]
    interface: bool,

    /// List every module and namespace
    #[arg(long, visible_alias = "mod", help_heading = "SYMBOL KINDS")]
    module: bool,

    /// List every macro
    #[arg(long = "macro", help_heading = "SYMBOL KINDS")]
    macros: bool,

    /// List every constant
    #[arg(long, visible_alias = "const", help_heading = "SYMBOL KINDS")]
    constant: bool,

    /// List every field
    #[arg(long, help_heading = "SYMBOL KINDS")]
    field: bool,

    /// List every variable
    #[arg(long, visible_alias = "var", help_heading = "SYMBOL KINDS")]
    variable: bool,

    /// List every type alias
    #[arg(
        long = "typedef",
        visible_alias = "type-alias",
        help_heading = "SYMBOL KINDS"
    )]
    types: bool,
}

impl Args {
    /// The symbol kinds the caller asked to list, in display order.
    fn kinds(&self) -> Vec<SymKind> {
        [
            (self.function, SymKind::Function),
            (self.method, SymKind::Method),
            (self.class, SymKind::Class),
            (self.interface, SymKind::Interface),
            (self.module, SymKind::Module),
            (self.macros, SymKind::Macro),
            (self.constant, SymKind::Constant),
            (self.field, SymKind::Field),
            (self.variable, SymKind::Variable),
            (self.types, SymKind::Type),
        ]
        .into_iter()
        .filter_map(|(on, kind)| on.then_some(kind))
        .collect()
    }

    /// The languages the caller asked for, empty when every one of them
    /// answers.
    fn langs(&self) -> Result<Vec<LangId>> {
        let mut langs = Vec::new();
        for name in &self.languages {
            let Some(lang) = LangId::of_name(name) else {
                let known: Vec<&str> = LangId::ALL.iter().map(|lang| lang.name()).collect();
                bail!("unknown language `{name}`; known: {}", known.join(" "));
            };
            if !langs.contains(&lang) {
                langs.push(lang);
            }
        }
        Ok(langs)
    }

    /// The cap on each group of sites, where `0` asks for all of them.
    fn limit(&self) -> usize {
        match self.limit {
            0 => usize::MAX,
            limit => limit,
        }
    }

    /// The record format a pipeline asked for.
    fn records(&self) -> Option<Records> {
        (self.vimgrep || self.matching_files).then_some(Records {
            paths_only: self.matching_files,
            end: if self.null { b'\0' } else { b'\n' },
        })
    }

    /// How far the walk reaches and how big a file it reads.
    fn opts(&self) -> Result<IndexOptions> {
        let mut opts = IndexOptions {
            hidden: self.hidden || self.unrestricted > 1,
            no_ignore: self.no_ignore || self.unrestricted > 0,
            ..Default::default()
        };
        if let Some(size) = &self.max_filesize {
            opts.max_file_bytes = bytes_of(size)?;
        }
        Ok(opts)
    }

    /// Source lines printed before and after each site.
    fn around(&self) -> (u32, u32) {
        (
            self.before_context.or(self.context).unwrap_or(0),
            self.after_context.or(self.context).unwrap_or(0),
        )
    }

    /// Split the positional arguments into a query and a root.
    ///
    /// `--index`, `--watch` and `--files` take no query, so their single
    /// positional is the directory when it names one. Everything else may be
    /// asked about a symbol called `src`, so only a positional written as a
    /// path (`./src`, `../other`) is read as one - a kind listing included,
    /// since `--class src` filters by name.
    fn targets(&self) -> Result<(Option<String>, PathBuf)> {
        if self.index || self.watch || self.files {
            let flag = match (self.index, self.watch) {
                (true, _) => "--index",
                (_, true) => "--watch",
                _ => "--files",
            };
            return match (&self.query, &self.root) {
                (Some(_), Some(_)) => bail!("{flag} takes at most one directory"),
                (Some(query), None) => Ok((None, PathBuf::from(query))),
                (None, root) => Ok((None, root.clone().unwrap_or_else(|| PathBuf::from(".")))),
            };
        }
        let query = self.query.as_deref().map(str::trim);
        let root = self.root.clone().unwrap_or_else(|| PathBuf::from("."));
        let named_as_path =
            |query: &str| query.contains(std::path::is_separator) || query == "." || query == "..";
        match query {
            Some(query)
                if self.root.is_none()
                    && !self.search
                    && named_as_path(query)
                    && Path::new(query).is_dir() =>
            {
                Ok((None, PathBuf::from(query)))
            }
            _ => Ok((query.map(str::to_string), root)),
        }
    }
}

fn main() -> ExitCode {
    let mut out = Out::new();
    let code = match answer(&mut out).and_then(|found| out.finish().map(|()| found)) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(err) => {
            eprintln!("rt: {err:#}");
            2
        }
    };
    let _ = io::stderr().flush();
    // Freeing tens of thousands of tags costs more than the query did, and the
    // kernel is about to reclaim all of it anyway.
    std::process::exit(code)
}

/// Print the answer, reporting whether anything matched.
fn answer(out: &mut Out) -> Result<bool> {
    if let Some(help) = agent_help() {
        write!(out, "{help}")?;
        return Ok(true);
    }
    let args = Args::parse();
    if args.skill {
        write!(out, "{SKILL}")?;
        return Ok(true);
    }
    App::new(args)?.run(out)
}

/// Buffered stdout that treats a reader going away as the end of the run.
///
/// `rt name | head` closes the pipe mid-answer: printing stops, the exit code
/// stays the one the answer earned, and every other write error reaches the
/// caller.
struct Out {
    out: BufWriter<StdoutLock<'static>>,
    closed: bool,
}

impl Out {
    fn new() -> Out {
        Out {
            out: BufWriter::new(io::stdout().lock()),
            closed: false,
        }
    }

    fn finish(&mut self) -> Result<()> {
        Ok(self.flush()?)
    }
}

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.closed {
            return Ok(buf.len());
        }
        match self.out.write(buf) {
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
                self.closed = true;
                Ok(buf.len())
            }
            written => written,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        match self.out.flush() {
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
                self.closed = true;
                Ok(())
            }
            flushed => flushed,
        }
    }
}

/// Whether a file at this path belongs to the answer.
type PathFilter = Box<dyn Fn(&str) -> bool>;

/// One run of `rt`: what was asked for, of which tree, printed how.
struct App {
    args: Args,
    style: Style,
    kinds: Vec<SymKind>,
    langs: Vec<LangId>,
    query: Option<String>,
    records: Option<Records>,
    globs: Option<PathFilter>,
    opts: IndexOptions,
    cache: Cache,
}

impl App {
    fn new(args: Args) -> Result<App> {
        let kinds = args.kinds();
        let langs = args.langs()?;
        let style = Style::new(args.color);
        let records = args.records();
        let opts = args.opts()?;
        let (query, root) = args.targets()?;
        let globs = globs_of(&root, &args.globs)?;
        Ok(App {
            args,
            style,
            kinds,
            langs,
            query,
            records,
            globs,
            cache: Cache::of(&root, &opts),
            opts,
        })
    }

    fn root(&self) -> &Path {
        self.cache.root()
    }

    /// The files an answer counts: the languages asked for, narrowed to the
    /// paths the globs admit.
    fn scope(&self) -> Scope<'_> {
        let scope = Scope::of(&self.langs);
        match &self.globs {
            Some(keep) => scope.paths(&**keep),
            None => scope,
        }
    }

    /// `false` means nothing matched, which the process reports as exit 1.
    fn run(&self, out: &mut Out) -> Result<bool> {
        if self.args.type_list {
            return self.list_types(out).map(|()| true);
        }
        if self.args.watch {
            return self.watch(out).map(|()| true);
        }
        if self.args.index {
            return self.build_index(out).map(|()| true);
        }
        if self.args.files {
            return self.list_files(out);
        }
        if self.query.is_none() && self.kinds.is_empty() && self.records.is_some() {
            bail!("a symbol to look for is required by --vimgrep and -l");
        }

        let trusted = !self.args.fresh && !self.args.no_cache;
        let cached = if self.args.no_cache {
            Some(SymbolIndex::of_dir(self.root(), &self.opts)?)
        } else if self.args.fresh {
            self.cache.resolve(&self.opts)?
        } else {
            self.cache.mapped()
        };
        let found = match cached {
            Some(index) => {
                let mut found = self.report(&index, out)?;
                if !found && trusted {
                    let checked = self.cache.query(&self.opts, Freshness::Checked)?;
                    found = self.report(&checked, out)?;
                    self.refresh_later(&checked);
                } else if !self.args.no_cache {
                    self.refresh_later(&index);
                }
                found
            }
            None => {
                let index = self.cold_index()?;
                let found = self.report(&index, out)?;
                self.spawn_build();
                found
            }
        };
        if !found {
            self.miss(out)?;
        }
        Ok(found)
    }

    /// `--type-list`: the languages this build can index, with the
    /// extensions each of them answers for.
    fn list_types(&self, out: &mut Out) -> Result<()> {
        for lang in LangId::ALL {
            let extensions = lang
                .extensions()
                .iter()
                .map(|ext| format!("*.{ext}"))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(out, "{}: {extensions}", lang.name())?;
        }
        Ok(())
    }

    /// `--files`: every file this tree offers the indexer, one per line.
    fn list_files(&self, out: &mut Out) -> Result<bool> {
        let mut paths: Vec<String> = {
            let scope = self.scope();
            Walk::new(self.root(), &self.opts)
                .meta()?
                .into_iter()
                .map(|file| file.path)
                .filter(|path| scope.admits(path))
                .collect()
        };
        paths.sort();
        let end = if self.args.null { b'\0' } else { b'\n' };
        for path in &paths {
            write!(out, "{}", self.style.paint(style::PATH, path))?;
            out.write_all(&[end])?;
        }
        Ok(!paths.is_empty())
    }

    /// Say something about the state of the index, unless the caller asked
    /// for silence.
    fn note(&self, note: &str) {
        if !self.args.no_messages && !self.args.quiet {
            eprintln!("note: {note}");
        }
    }

    /// Answer with no index to answer from, and say so: a name narrows the
    /// scan to the files that can hold it, a kind listing needs every file.
    fn cold_index(&self) -> Result<SymbolIndex> {
        let (Some(query), true) = (&self.query, self.kinds.is_empty()) else {
            self.note("no index yet - scanned the whole tree; it is building in background");
            return SymbolIndex::of_dir(self.root(), &self.opts);
        };
        self.note(&format!(
            "no index yet - answered from a targeted scan; full index building in background{}",
            if self.args.search {
                "; case-variant names may be missing until it lands"
            } else {
                ""
            }
        ));
        let needle = if self.args.search {
            query.as_str()
        } else {
            Symbol::parse(query).name
        };
        self.targeted_scan(needle)
    }

    /// Index only the files that can possibly hold `needle`.
    ///
    /// An occurrence of an identifier is a run of its own bytes, so a file
    /// whose contents do not contain `needle` cannot contain the symbol: the
    /// answer for that name is exact, and the files nobody asked about are
    /// never parsed. A search that ignores case knows no such run, so it
    /// reads the tree.
    fn targeted_scan(&self, needle: &str) -> Result<SymbolIndex> {
        let mut files = Walk::new(self.root(), &self.opts).files()?;
        if !needle.is_empty() && !self.args.ignore_case {
            let finder = memchr::memmem::Finder::new(needle.as_bytes());
            files.retain(|file| finder.find(&file.bytes).is_some());
        }
        Ok(SymbolIndex::build(files, &self.opts))
    }

    /// `--watch`: hold the index of the tree to the tree until interrupted.
    fn watch(&self, out: &mut Out) -> Result<()> {
        watch::run(self.root(), &self.opts, |progress| {
            let printed = match progress {
                watch::Progress::Watching { directories } => writeln!(
                    out,
                    "watching {} ({directories} directories)",
                    self.root().display()
                ),
                watch::Progress::Polling { every } => writeln!(
                    out,
                    "watching {} every {}s - the kernel has no file watches left",
                    self.root().display(),
                    every.as_secs_f32().round()
                ),
                watch::Progress::Refreshed {
                    files,
                    reparsed,
                    occurrences,
                    took,
                } => writeln!(
                    out,
                    "indexed {files} files ({reparsed} reparsed), {occurrences} occurrences in {}ms",
                    took.as_millis()
                ),
            };
            // A watcher runs until it is interrupted, so each line has to
            // reach the reader as it happens.
            let _ = printed.and_then(|()| out.flush());
        })
    }

    /// `--index`: build the index and persist it, keeping the records of
    /// every file that has not changed since the last build.
    fn build_index(&self, out: &mut Out) -> Result<()> {
        let index = if self.args.no_cache {
            SymbolIndex::of_dir(self.root(), &self.opts)?
        } else {
            self.cache.resolve_or_scan(&self.opts)?
        };
        if !self.args.no_cache
            && index.drifted()
            && let Some(_lock) = self.cache.try_lock()
        {
            self.cache.store(&index)?;
        }
        let stats = index.stats();
        let report = IndexReport {
            stats,
            symbols: index.symbol_count(),
        };
        if self.args.json {
            writeln!(out, "{}", self.style.json(&report)?)?;
        } else {
            writeln!(
                out,
                "indexed {} files ({} skipped, {} reused), {} symbols, {} occurrences in {}ms",
                stats.files,
                stats.skipped,
                stats.reused,
                report.symbols,
                stats.occurrences,
                stats.duration_ms
            )?;
        }
        Ok(())
    }

    /// Bring the index up to date after answering, never during.
    ///
    /// A query that trusted the index still nudges it, so a tree nobody
    /// watches converges on its own instead of drifting until someone runs
    /// `--index`.
    fn refresh_later(&self, index: &SymbolIndex) {
        if index.drifted() || self.cache.refresh_due() {
            self.spawn_build();
        }
    }

    /// Run `rt --index` on the tree in the background, detached from this
    /// process.
    ///
    /// Nothing is spawned when a watcher already owns the tree, or when
    /// another builder holds the lock: both are about to write the index
    /// anyway.
    fn spawn_build(&self) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        if self.cache.watcher().is_some() {
            return;
        }
        let Some(lock) = self.cache.try_lock() else {
            return;
        };
        drop(lock);
        let mut build = Command::new(exe);
        build.arg("--index").arg(self.root());
        if self.opts.hidden {
            build.arg("--hidden");
        }
        if self.opts.no_ignore {
            build.arg("--no-ignore");
        }
        if self.opts.max_file_bytes != IndexOptions::default().max_file_bytes {
            build.arg(format!("--max-filesize={}", self.opts.max_file_bytes));
        }
        let _ = build
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    /// Every site of `sym` the filters admit, capped at `limit` per group.
    ///
    /// A filter narrows what the index answered, so the cap has to wait for
    /// it: the whole set is collected first and capped afterwards.
    fn hits<'i>(&self, index: &'i SymbolIndex, sym: &str, limit: usize) -> Option<SymbolHits<'i>> {
        let filtered = self.globs.is_some() || !self.langs.is_empty();
        let lookup_limit = if filtered { usize::MAX } else { limit };
        let mut hits = match self.args.ignore_case {
            true => index.lookup_any_case(sym, lookup_limit)?,
            false => index.lookup(sym, lookup_limit)?,
        };
        if filtered {
            let scope = self.scope();
            hits.filter(|site| scope.admits(site.path), limit);
            if hits.counts.total() == 0 {
                return None;
            }
        }
        Some(hits)
    }

    /// Whether `index` holds anything for the query, printing none of it.
    fn found(&self, index: &SymbolIndex) -> bool {
        if self.args.search || !self.kinds.is_empty() {
            return !index
                .list(&self.kinds, self.scope(), self.query.as_deref())
                .is_empty();
        }
        match self.query.as_deref() {
            Some(query) => self.hits(index, query, 1).is_some(),
            None => index.overview(self.scope()).files > 0,
        }
    }

    /// Print what `index` holds for the query.
    ///
    /// `false` means it held nothing and nothing was printed: the caller
    /// decides whether that is worth saying yet.
    fn report(&self, index: &SymbolIndex, out: &mut Out) -> Result<bool> {
        if self.args.quiet {
            return Ok(self.found(index));
        }
        if self.args.search || !self.kinds.is_empty() {
            return self.list(index, out);
        }
        let Some(query) = self.query.as_deref() else {
            return self.overview(index, out);
        };

        let Some(mut hits) = self.hits(index, query, self.args.limit()) else {
            return Ok(false);
        };

        if let Some(records) = self.records {
            let mut records = RecordWriter::new(records, &self.style, self.root(), out);
            let groups = [
                (&hits.definitions, !self.args.references),
                (&hits.declarations, !self.args.references),
                (&hits.implementations, !self.args.definitions),
                (&hits.references, !self.args.definitions),
            ];
            for (sites, wanted) in groups {
                if !wanted {
                    continue;
                }
                for site in sites {
                    records.site(site)?;
                }
            }
            return Ok(records.wrote());
        }

        if self.args.json {
            writeln!(out, "{}", self.style.json(&hits)?)?;
            return Ok(true);
        }

        // The symbol is in the index but the kind filter hides all of it: say
        // what the other half holds instead of printing a bare header.
        let defined = hits.counts.definitions + hits.counts.declarations;
        let used = hits.counts.references + hits.counts.implementations;
        if self.args.definitions && defined == 0 {
            let plural = if used == 1 { "" } else { "s" };
            writeln!(
                out,
                "no definitions of `{}`; {used} reference{plural} (drop --definitions)",
                hits.pretty
            )?;
            return Ok(true);
        }
        if self.args.references && used == 0 {
            let plural = if defined == 1 { "" } else { "s" };
            writeln!(
                out,
                "no references of `{}`; {defined} definition{plural} (drop --refs)",
                hits.pretty
            )?;
            return Ok(true);
        }

        writeln!(
            out,
            "{}{}{}",
            self.style.paint(style::HEADING, &hits.pretty),
            self.style.gap(),
            self.style.paint(style::KIND, hits.kind.name())
        )?;
        let mut sources = Sources::new(self.root());
        let (before, after) = self.args.around();
        let (defs, refs) = (!self.args.references, !self.args.definitions);
        let titles = match self.style.compact {
            true => ["def", "decl", "impl", "ref"],
            false => [
                "definitions",
                "declarations",
                "implementations",
                "references",
            ],
        };
        let groups = titles.into_iter().zip([defs, defs, refs, refs]);
        for ((sites, total), (title, wanted)) in hits.groups().into_iter().zip(groups) {
            if !wanted || sites.is_empty() {
                continue;
            }
            writeln!(out, "{}", self.style.heading(title, *total, sites.len()))?;
            for site in sites.iter() {
                let first = site.line.saturating_sub(before).max(1);
                self.context_lines(&mut sources, site.path, first, site.line - 1, out)?;
                write!(
                    out,
                    "{}{}:{}:{}{}",
                    self.style.indent(),
                    self.style.paint(style::PATH, site.path),
                    self.style.paint(style::LINE, site.line),
                    site.col + 1,
                    self.style.gap()
                )?;
                if let Some(context) = site.context {
                    write!(
                        out,
                        "{} ",
                        self.style
                            .paint(style::CONTEXT, format_args!("[{context}]"))
                    )?;
                }
                let line = sources.line(site.path, site.line).unwrap_or_default();
                writeln!(out, "{}", self.style.source(line, site.col, site.len))?;
                self.context_lines(
                    &mut sources,
                    site.path,
                    site.line + 1,
                    site.line + after,
                    out,
                )?;
            }
        }
        Ok(true)
    }

    /// Print the source lines of a path from `first` to `last`, the way
    /// ripgrep prints context: `-` in place of the `:` of a site.
    fn context_lines(
        &self,
        sources: &mut Sources,
        path: &str,
        first: u32,
        last: u32,
        out: &mut Out,
    ) -> Result<()> {
        for row in first..=last {
            let Some(text) = sources.line(path, row) else {
                continue;
            };
            writeln!(
                out,
                "{}{}-{}-{}{text}",
                self.style.indent(),
                self.style.paint(style::PATH, path),
                self.style.paint(style::LINE, row),
                self.style.gap()
            )?;
        }
        Ok(())
    }

    /// What the tree holds, for a caller who has not named a symbol yet:
    /// how much code there is, in which languages, and what kind of symbols
    /// it is made of.
    fn overview(&self, index: &SymbolIndex, out: &mut Out) -> Result<bool> {
        let overview = index.overview(self.scope());
        if overview.files == 0 {
            return Ok(false);
        }
        if self.args.json {
            writeln!(out, "{}", self.style.json(&overview)?)?;
            return Ok(true);
        }
        let gap = self.style.gap();
        let root = self.root().canonicalize();
        let root = root.as_deref().unwrap_or(self.root());
        writeln!(
            out,
            "{}{gap}{} files, {} symbols, {} occurrences",
            self.style.paint(style::HEADING, root.display()),
            overview.files,
            overview.symbols,
            overview.occurrences
        )?;
        let languages = overview
            .languages
            .iter()
            .map(|lang| {
                format!(
                    "{} {}",
                    self.style.paint(style::KIND, lang.language),
                    lang.files
                )
            })
            .collect::<Vec<_>>()
            .join(gap);
        let kinds = overview
            .kinds
            .iter()
            .map(|kind| {
                format!(
                    "{} {}",
                    self.style.paint(style::KIND, kind.kind.name()),
                    kind.symbols
                )
            })
            .collect::<Vec<_>>()
            .join(gap);
        let try_these = [
            "rt <symbol>",
            "rt <part> --search",
            "rt --class",
            "rt --help",
        ]
        .join(if self.style.compact { "|" } else { gap });
        for (label, row) in [
            ("languages", languages),
            ("kinds", kinds),
            ("try", try_these),
        ] {
            let label = self.style.paint(style::HEADING, label);
            match self.style.compact {
                true => writeln!(out, "{label} {row}")?,
                false => writeln!(out, "{label:<11}{row}")?,
            }
        }
        Ok(true)
    }

    /// List symbol names: `--search`, or one or more kind flags.
    fn list(&self, index: &SymbolIndex, out: &mut Out) -> Result<bool> {
        let query = self.query.as_deref();
        let hits = index.list(&self.kinds, self.scope(), query);
        if hits.is_empty() {
            return Ok(false);
        }
        let shown = hits.len().min(self.args.limit());
        if let Some(records) = self.records {
            let mut records = RecordWriter::new(records, &self.style, self.root(), out);
            for hit in &hits[..shown] {
                if let Some(found) = self.hits(index, &hit.sym, 1)
                    && let Some(site) = found.best()
                {
                    records.site(site)?;
                }
            }
            return Ok(records.wrote());
        }
        if self.args.json {
            writeln!(out, "{}", self.style.json(&&hits[..shown])?)?;
            return Ok(true);
        }
        writeln!(
            out,
            "{}",
            self.style.heading(&self.label(), hits.len(), shown)
        )?;
        for hit in &hits[..shown] {
            let name = self.style.found(&hit.pretty, query);
            let kind = self.style.paint(style::KIND, hit.kind.name());
            let (defs, refs) = (hit.definitions, hit.references);
            if self.style.compact {
                writeln!(out, "{name} {kind} {defs} defs, {refs} refs")?;
                continue;
            }
            let pad = 46usize.saturating_sub(hit.pretty.chars().count());
            writeln!(
                out,
                "  {name}{:pad$} {kind:<10} {defs} defs, {refs} refs",
                ""
            )?;
        }
        Ok(true)
    }

    /// Say that nothing matched, once the caller is sure of it.
    fn miss(&self, out: &mut Out) -> Result<()> {
        // A pipeline reads records, not prose: the exit code carries the miss.
        if self.records.is_some() || self.args.quiet {
            return Ok(());
        }
        let query = self.query.as_deref();
        if self.args.search || !self.kinds.is_empty() {
            if self.args.json {
                writeln!(out, "[]")?;
                return Ok(());
            }
            let label = self.style.paint(style::HEADING, self.label());
            let langs = self.in_langs();
            match query {
                Some(query) => writeln!(
                    out,
                    "no {label}{langs} whose name or owner contains `{query}`"
                )?,
                None => writeln!(out, "no {label}{langs} in index")?,
            }
            return Ok(());
        }
        let Some(query) = query else {
            match self.args.json {
                true => writeln!(out, "{{}}")?,
                false => writeln!(
                    out,
                    "no{} source rt can index under {}",
                    self.in_langs(),
                    self.root().display()
                )?,
            }
            return Ok(());
        };
        let narrowing = self.narrowing();
        match narrowing.is_empty() {
            true => writeln!(
                out,
                "no symbol `{query}` in index; try `rt {query} --search`"
            )?,
            false => writeln!(out, "no occurrence of `{query}`{narrowing}")?,
        }
        Ok(())
    }

    /// The languages the caller named, as ` in rust/ts`, or nothing.
    fn in_langs(&self) -> String {
        if self.langs.is_empty() {
            return String::new();
        }
        let names: Vec<&str> = self.langs.iter().map(|lang| lang.name()).collect();
        format!(" in {}", names.join("/"))
    }

    /// How a query was narrowed, for a miss that owes the caller a reason.
    fn narrowing(&self) -> String {
        let mut narrowing = self.in_langs();
        if !self.args.globs.is_empty() {
            narrowing.push_str(&format!(" matching `{}`", self.args.globs.join("` `")));
        }
        narrowing
    }

    /// What a listing calls the things it lists.
    fn label(&self) -> String {
        if self.kinds.is_empty() {
            return "matches".to_string();
        }
        self.kinds
            .iter()
            .map(|kind| match kind {
                SymKind::Class => "classes".to_string(),
                SymKind::Unknown => "symbols".to_string(),
                kind => format!("{}s", kind.name()),
            })
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// The path filter `-g` asked for, or `None` when it asked for nothing.
///
/// Globs read as they do in a `gitignore` file with the sense of `!`
/// inverted, which is what ripgrep does: `-g 'src/**'` keeps that subtree,
/// `-g '!**/tests/**'` drops one. A glob naming a directory keeps the files
/// under it, so `-g src/api` reads as the prefix it looks like.
fn globs_of(root: &Path, patterns: &[String]) -> Result<Option<PathFilter>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = OverrideBuilder::new(root);
    for pattern in patterns {
        builder
            .add(pattern)
            .with_context(|| format!("bad glob `{pattern}`"))?;
        let (bang, bare) = match pattern.strip_prefix('!') {
            Some(bare) => ("!", bare),
            None => ("", pattern.as_str()),
        };
        if !bare.contains(['*', '?', '[', '{']) {
            let under = format!("{bang}{}/**", bare.trim_end_matches('/'));
            builder
                .add(&under)
                .with_context(|| format!("bad glob `{pattern}`"))?;
        }
    }
    let globs: Override = builder.build().context("build the glob filter")?;
    Ok(Some(Box::new(move |path: &str| {
        !globs.matched(path, false).is_ignore()
    })))
}

/// A size in bytes, written plainly or with a `K`, `M` or `G` suffix.
fn bytes_of(size: &str) -> Result<usize> {
    let size = size.trim();
    let (digits, scale) = match size.as_bytes().last().map(u8::to_ascii_uppercase) {
        Some(b'K') => (&size[..size.len() - 1], 1 << 10),
        Some(b'M') => (&size[..size.len() - 1], 1 << 20),
        Some(b'G') => (&size[..size.len() - 1], 1 << 30),
        _ => (size, 1),
    };
    let count: usize = digits
        .trim()
        .parse()
        .with_context(|| format!("bad size `{size}`"))?;
    count
        .checked_mul(scale)
        .with_context(|| format!("size `{size}` does not fit"))
}

/// `--index` output: the stats plus the symbol count, which costs a pass.
#[derive(Serialize)]
struct IndexReport<'a> {
    #[serde(flatten)]
    stats: &'a IndexStats,
    symbols: usize,
}

/// Source lines are read on demand: the index deliberately does not store
/// them, and the line arrives untrimmed so a column still points at a
/// character of it.
struct Sources<'a> {
    root: &'a Path,
    files: HashMap<String, Option<String>>,
}

impl<'a> Sources<'a> {
    fn new(root: &'a Path) -> Self {
        Sources {
            root,
            files: HashMap::new(),
        }
    }

    fn line(&mut self, path: &str, line: u32) -> Option<&str> {
        let source = self
            .files
            .entry(path.to_string())
            .or_insert_with(|| std::fs::read_to_string(self.root.join(path)).ok());
        source.as_deref()?.lines().nth(line as usize - 1)
    }
}

/// The record format a pipeline asked for.
#[derive(Copy, Clone)]
struct Records {
    /// Print the path by itself, for `xargs`.
    paths_only: bool,
    /// What ends a record: a newline, or NUL for `xargs -0`.
    end: u8,
}

/// Sites as one record each: `path:line:col:text`, or the path alone.
///
/// A site is printed once, so a line two symbols reach does not reach an
/// editor twice.
struct RecordWriter<'a, 'i> {
    records: Records,
    style: &'a Style,
    sources: Sources<'a>,
    seen: HashSet<(&'i str, u32, u32)>,
    out: &'a mut Out,
    wrote: bool,
}

impl<'a, 'i> RecordWriter<'a, 'i> {
    fn new(records: Records, style: &'a Style, root: &'a Path, out: &'a mut Out) -> Self {
        RecordWriter {
            records,
            style,
            sources: Sources::new(root),
            seen: HashSet::new(),
            out,
            wrote: false,
        }
    }

    fn site(&mut self, site: &SymbolSite<'i>) -> Result<()> {
        let key = match self.records.paths_only {
            true => (site.path, 0, 0),
            false => (site.path, site.line, site.col),
        };
        if !self.seen.insert(key) {
            return Ok(());
        }
        self.wrote = true;
        let path = self.style.paint(style::PATH, site.path);
        if self.records.paths_only {
            write!(self.out, "{path}")?;
        } else {
            let line = self.sources.line(site.path, site.line).unwrap_or_default();
            let text = self.style.source(line, site.col, site.len);
            write!(
                self.out,
                "{path}:{}:{}:{text}",
                self.style.paint(style::LINE, site.line),
                site.col + 1
            )?;
        }
        let end = self.records.end;
        Ok(self.out.write_all(&[end])?)
    }

    /// Whether any site was printed.
    fn wrote(&self) -> bool {
        self.wrote
    }
}
