use std::fs::{File, TryLockError};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use crate::index::{IndexOptions, SymbolIndex};
use crate::tagfile::TagFile;
use crate::walk::{FileMeta, Walk};

/// How long an index is trusted before something walks the tree again.
pub const CHECK_EVERY: Duration = Duration::from_secs(30);

/// How current an answer has to be.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Freshness {
    /// Answer from the index as it stands, building one only when there is
    /// none. Edits made since it was written are invisible, which is what
    /// makes the answer cost milliseconds.
    Index,
    /// Walk the tree first and reparse whatever changed.
    Checked,
}

/// A claim on the index of a tree: the right to build it, or to keep it
/// current as its watcher.
///
/// The claim is a lock the kernel owns, so a holder that crashes or is
/// killed releases it without leaving anything behind to clean up.
pub struct Lock {
    _file: File,
}

/// The cached index of one tree: where it lives, who holds it, and how
/// current it is.
///
/// The cache path is derived once, so a caller that locks, resolves and
/// stores hashes the root once rather than on every call.
#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
    path: PathBuf,
}

impl Cache {
    /// The cache entry of `root` under `opts`.
    ///
    /// A walk that reaches further than the default one - hidden files,
    /// ignored files, a larger size limit - describes a different tree, so
    /// it gets an index of its own instead of overwriting the usual one.
    pub fn of(root: &Path, opts: &IndexOptions) -> Cache {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut hasher = Fnv::default();
        hasher.write(canonical.as_os_str().as_encoded_bytes());
        if opts != &IndexOptions::default() {
            opts.hash(&mut hasher);
        }
        Cache {
            root: root.to_path_buf(),
            path: cache_dir().join(format!("{:016x}.tags", hasher.finish())),
        }
    }

    /// The tree this index describes.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the index lives.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Map the index, or `None` when there is none this build can read.
    pub fn load(&self) -> Option<TagFile> {
        TagFile::open(&self.path)
    }

    /// The index as it stands.
    ///
    /// This is the fast answer: no walk, no parse, no check that the tree
    /// still looks like the index says. [`Cache::resolve`] is the checked one.
    pub fn mapped(&self) -> Option<SymbolIndex> {
        SymbolIndex::mapped_at(&self.path)
    }

    /// Write `index` to the cache.
    pub fn store(&self, index: &SymbolIndex) -> Result<()> {
        index.store_to(&self.path)
    }

    /// Record that the tree was compared against the index just now.
    ///
    /// The index file's own timestamp carries this, so a check that found
    /// nothing to do leaves no other trace.
    pub fn mark_checked(&self) {
        if let Ok(file) = File::options().write(true).open(&self.path) {
            let _ = file.set_modified(SystemTime::now());
        }
    }

    /// Was the tree compared against the index within `age`?
    pub fn checked_within(&self, age: Duration) -> bool {
        std::fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .is_ok_and(|checked| {
                SystemTime::now()
                    .duration_since(checked)
                    .is_ok_and(|since| since < age)
            })
    }

    /// Claim the right to build the index, unless another process has it.
    pub fn try_lock(&self) -> Option<Lock> {
        let file = hold(&self.path.with_extension("lock")).ok()??;
        Some(Lock { _file: file })
    }

    /// Claim the tree for this watcher, or `None` when another one already
    /// has it.
    pub fn claim_watch(&self) -> Result<Option<Lock>> {
        let Some(file) = hold(&self.watch_path())? else {
            return Ok(None);
        };
        std::fs::write(self.pid_path(), std::process::id().to_string()).ok();
        Ok(Some(Lock { _file: file }))
    }

    /// The process id of the watcher keeping the tree current, if one is
    /// running.
    pub fn watcher(&self) -> Option<u32> {
        let file = File::options()
            .read(true)
            .write(true)
            .open(self.watch_path())
            .ok()?;
        match file.try_lock() {
            Err(TryLockError::WouldBlock) => Some(
                std::fs::read_to_string(self.pid_path())
                    .ok()
                    .and_then(|pid| pid.trim().parse().ok())
                    .unwrap_or_default(),
            ),
            _ => None,
        }
    }

    fn watch_path(&self) -> PathBuf {
        self.path.with_extension("watch")
    }

    /// The pid of the watcher lives beside its claim, not inside it: a claim
    /// held exclusively is unreadable to everyone but its holder on Windows.
    fn pid_path(&self) -> PathBuf {
        self.path.with_extension("watchpid")
    }

    /// Answer from the index, reparsing only the files that changed.
    ///
    /// Returns `None` when the tree has no index yet.
    pub fn resolve(&self, opts: &IndexOptions) -> Result<Option<SymbolIndex>> {
        let started = Instant::now();
        let Some(mapped) = self.load() else {
            return Ok(None);
        };

        // One binary search per walked file: a file whose record still matches
        // keeps it, any other file is parsed again.
        let classify = |meta: FileMeta| match mapped
            .find_file(&meta.path)
            .filter(|_| meta.mtime_ns != 0)
            .filter(|(_, file)| file.mtime_ns() == meta.mtime_ns && file.size() == meta.size)
        {
            Some((slot, _)) => Ok(slot),
            None => Err(meta),
        };
        let walked = Walk::new(&self.root, opts).meta()?;
        // The searches are independent, so they run on every core.
        #[cfg(feature = "rayon")]
        let (kept, changed): (Vec<usize>, Vec<FileMeta>) = {
            use rayon::iter::Either;
            use rayon::prelude::*;

            walked
                .into_par_iter()
                .partition_map(|meta| match classify(meta) {
                    Ok(slot) => Either::Left(slot),
                    Err(meta) => Either::Right(meta),
                })
        };
        #[cfg(not(feature = "rayon"))]
        let (kept, changed): (Vec<usize>, Vec<FileMeta>) = {
            let (mut kept, mut changed) = (Vec::new(), Vec::new());
            for meta in walked {
                match classify(meta) {
                    Ok(slot) => kept.push(slot),
                    Err(meta) => changed.push(meta),
                }
            }
            (kept, changed)
        };
        let mut live = vec![false; mapped.file_count()];
        for slot in &kept {
            live[*slot] = true;
        }

        let (fresh, skipped) =
            SymbolIndex::tag_files(changed.into_iter().filter_map(FileMeta::read), opts);
        let index =
            SymbolIndex::from_mapped(mapped, live, fresh, skipped, kept.len(), started.elapsed());
        if !index.drifted() {
            self.mark_checked();
        }
        Ok(Some(index))
    }

    /// The index after checking the tree, built from nothing when there is no
    /// index yet.
    pub fn resolve_or_scan(&self, opts: &IndexOptions) -> Result<SymbolIndex> {
        match self.resolve(opts)? {
            Some(index) => Ok(index),
            None => SymbolIndex::of_dir(&self.root, opts),
        }
    }

    /// The index, as current as `how` asks for.
    ///
    /// An index that had to be built, or brought up to the tree, is written
    /// back when this process can take the lock, so the next caller maps it
    /// instead of parsing again.
    pub fn query(&self, opts: &IndexOptions, how: Freshness) -> Result<SymbolIndex> {
        if how == Freshness::Index
            && let Some(index) = self.mapped()
        {
            return Ok(index);
        }
        let index = self.resolve_or_scan(opts)?;
        if index.drifted()
            && let Some(_lock) = self.try_lock()
        {
            // An index that cannot be written only costs time: whoever holds
            // the lock is about to write the same records.
            let _ = self.store(&index);
        }
        Ok(index)
    }

    /// Whether the tree is due a walk: it is not while a watcher keeps the
    /// index current, nor while something checked it within [`CHECK_EVERY`].
    pub fn refresh_due(&self) -> bool {
        self.watcher().is_none() && !self.checked_within(CHECK_EVERY)
    }

    /// Bring the index up to the tree on a background thread.
    ///
    /// Returns at once, and does nothing when the tree is not due a walk or
    /// when another process is already building it. The thread dies with the
    /// process, so a command that answers and exits wants a detached builder
    /// instead.
    pub fn refresh(&self, opts: &IndexOptions) {
        if !self.refresh_due() {
            return;
        }
        let (cache, opts) = (self.clone(), opts.clone());
        thread::spawn(move || {
            let Some(_lock) = cache.try_lock() else {
                return;
            };
            if let Ok(index) = cache.resolve_or_scan(&opts)
                && index.drifted()
            {
                let _ = cache.store(&index);
            }
        });
    }
}

fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("riptags")
}

/// FNV-1a, because the name of a cache entry has to survive a toolchain
/// upgrade: `DefaultHasher` promises nothing across releases, and every
/// entry it named would be orphaned with nothing to reap it.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
}

/// Lock `path` for the life of the returned file, or `None` when another
/// process holds it.
fn hold(path: &Path) -> Result<Option<File>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("lock {}", path.display()))
        }
    }
}
