//! Keep the index of a tree current while the tree is being edited.
//!
//! Queries trust the index, so something has to keep it honest. This is that
//! something: after each burst of edits settles, the files that changed are
//! reparsed and the index is written back.
//!
//! The kernel reports the edits where it can. A recursive watch costs one
//! watch per directory, which a large tree can exhaust, so the loop falls
//! back to looking for itself on a timer.

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{ErrorKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::cache::{CHECK_EVERY, Cache};
use crate::index::IndexOptions;
use crate::lang::LangId;
use crate::walk::Walk;

/// How long the tree must be quiet before a burst of edits is indexed.
const SETTLE: Duration = Duration::from_millis(400);

/// How long a burst may hold off indexing, however busy the tree stays.
const PATIENCE: Duration = Duration::from_secs(5);

/// What the watcher is doing, for whoever asked it to watch.
pub enum Progress {
    /// The kernel is reporting edits on this many directories.
    Watching { directories: usize },
    /// The kernel had no watches left, so the tree is checked on a timer.
    Polling { every: Duration },
    /// The index was written again.
    Refreshed {
        files: usize,
        /// How many of those files this refresh had to parse again.
        reparsed: usize,
        occurrences: usize,
        took: Duration,
    },
}

/// Index `root`, then keep indexing it until the process is interrupted.
///
/// Only one watcher owns a tree at a time; a second one returns an error
/// rather than duplicating the first one's work.
pub fn run(root: &Path, opts: &IndexOptions, report: impl FnMut(Progress)) -> Result<()> {
    // The kernel reports an edit under the path the watch was placed on,
    // resolved: against a relative root every event would look foreign.
    let root = root
        .canonicalize()
        .with_context(|| format!("watch {}", root.display()))?;
    let cache = Cache::of(&root, opts);
    let Some(_claim) = cache.claim_watch()? else {
        let pid = cache.watcher().unwrap_or_default();
        anyhow::bail!("{} is already watched by pid {pid}", root.display());
    };
    Watch {
        cache,
        opts,
        report,
    }
    .run()
}

/// One watched tree: the claim on its index, and how the loop hears about
/// its edits.
struct Watch<'a, R: FnMut(Progress)> {
    cache: Cache,
    opts: &'a IndexOptions,
    report: R,
}

impl<R: FnMut(Progress)> Watch<'_, R> {
    fn root(&self) -> &Path {
        self.cache.root()
    }

    fn run(&mut self) -> Result<()> {
        self.refresh()?;
        let mut watched = self.watch_tree()?;
        (self.report)(match &watched {
            Some(watched) => Progress::Watching {
                directories: watched.directories,
            },
            None => Progress::Polling { every: CHECK_EVERY },
        });

        loop {
            let Some(watched) = &mut watched else {
                std::thread::sleep(CHECK_EVERY);
                self.refresh()?;
                continue;
            };
            if !self.wait_for_edits(&watched.events, &watched.ignored) {
                return Ok(());
            }
            self.refresh()?;
            // A burst can have created directories, and a directory nobody
            // watches hides every edit inside it. It can also have written a
            // `.gitignore`, which decides what wakes the loop.
            watched.ignored = self.ignores();
            watched.directories = self
                .arm(&mut watched.watcher)
                .unwrap_or(watched.directories);
        }
    }

    /// Reparse whatever changed, write the index back if anything did, and
    /// report it.
    fn refresh(&mut self) -> Result<()> {
        let started = Instant::now();
        let index = self.cache.resolve_or_scan(self.opts)?;
        if !index.drifted() {
            return Ok(());
        }
        if let Some(_lock) = self.cache.try_lock() {
            self.cache.store(&index)?;
            self.cache.mark_checked();
            (self.report)(Progress::Refreshed {
                files: index.stats().files,
                reparsed: index.fresh_files().len(),
                occurrences: index.stats().occurrences,
                took: started.elapsed(),
            });
        }
        Ok(())
    }

    /// Put a watch on every directory of the tree, or `None` when the kernel
    /// has no watches left to give.
    fn watch_tree(&self) -> Result<Option<Watched>> {
        let (tx, rx) = channel();
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
            let _ = tx.send(event);
        })
        .context("start a file watcher")?;
        match self.arm(&mut watcher) {
            Ok(directories) => Ok(Some(Watched {
                watcher,
                events: rx,
                ignored: self.ignores(),
                directories,
            })),
            Err(error) if matches!(error.kind, ErrorKind::MaxFilesWatch) => Ok(None),
            Err(error) => Err(error).with_context(|| format!("watch {}", self.root().display())),
        }
    }

    /// Watch each directory the index walk reaches, ignored ones excluded: a
    /// build writing into the tree must not wake the watcher on every object
    /// file, and a widened walk (`--hidden`, `--no-ignore`) must hear about
    /// the files it indexes.
    fn arm(&self, watcher: &mut RecommendedWatcher) -> notify::Result<usize> {
        let mut directories = 0;
        let walk = Walk::new(self.root(), self.opts).builder().build();
        for entry in walk.flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
                continue;
            }
            match watcher.watch(entry.path(), RecursiveMode::NonRecursive) {
                Ok(()) => directories += 1,
                // The directory went away between the walk and the watch.
                Err(error) if matches!(error.kind, ErrorKind::PathNotFound | ErrorKind::Io(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(directories)
    }

    /// Block until the tree has changed and then gone quiet again.
    ///
    /// `false` means the watcher is gone and there is nothing left to wait
    /// for.
    fn wait_for_edits(
        &self,
        events: &Receiver<notify::Result<notify::Event>>,
        ignored: &Gitignore,
    ) -> bool {
        let mut burst: Option<Instant> = None;
        loop {
            let timeout = match burst {
                Some(started) => SETTLE.min(PATIENCE.saturating_sub(started.elapsed())),
                None => Duration::from_secs(3600),
            };
            match events.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    if self.indexable(&event, ignored) {
                        burst.get_or_insert_with(Instant::now);
                    }
                }
                Ok(Err(_)) => {}
                Err(RecvTimeoutError::Timeout) if burst.is_some() => return true,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return false,
            }
        }
    }

    /// Would this event change what the index holds?
    fn indexable(&self, event: &notify::Event, ignored: &Gitignore) -> bool {
        if matches!(event.kind, EventKind::Access(_)) {
            return false;
        }
        event.paths.iter().any(|path| {
            let Some(relative) = path
                .strip_prefix(self.root())
                .ok()
                .and_then(|path| path.to_str())
            else {
                return false;
            };
            LangId::of_path(relative).is_some()
                && !self.opts.excludes(relative)
                && !ignored.matched_path_or_any_parents(path, false).is_ignore()
        })
    }

    /// The ignore rules of the tree, as far as one file of them goes: the
    /// tree walk applies the rest, this only keeps obvious noise from waking
    /// the loop. `--no-ignore` keeps none of them.
    fn ignores(&self) -> Gitignore {
        if self.opts.no_ignore {
            return Gitignore::empty();
        }
        let mut builder = GitignoreBuilder::new(self.root());
        for name in [".gitignore", ".ignore", ".git/info/exclude"] {
            builder.add(self.root().join(name));
        }
        builder.build().unwrap_or_else(|_| Gitignore::empty())
    }
}

/// A tree the kernel reports edits on.
struct Watched {
    watcher: RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
    ignored: Gitignore,
    directories: usize,
}
