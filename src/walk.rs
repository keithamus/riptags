use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::index::{IndexOptions, SourceFile};

/// One indexable file found on disk, without its contents.
#[derive(Clone, Debug)]
pub struct FileMeta {
    /// Path relative to the walked root, `/`-separated.
    pub path: String,
    pub abs: PathBuf,
    pub mtime_ns: i64,
    pub size: u64,
}

impl FileMeta {
    /// Read the file, or `None` when it went away since the walk.
    pub fn read(self) -> Option<SourceFile> {
        let bytes = std::fs::read(&self.abs).ok()?;
        Some(SourceFile {
            path: self.path,
            mtime_ns: self.mtime_ns,
            size: self.size,
            bytes,
        })
    }
}

/// A walk of one tree under one set of limits.
pub struct Walk<'a> {
    root: &'a Path,
    opts: &'a IndexOptions,
}

impl<'a> Walk<'a> {
    pub fn new(root: &'a Path, opts: &'a IndexOptions) -> Walk<'a> {
        Walk { root, opts }
    }

    /// List every indexable file, without reading any of them.
    ///
    /// This is the cheap walk that change detection uses.
    pub fn meta(&self) -> Result<Vec<FileMeta>> {
        if !std::fs::metadata(self.root)
            .with_context(|| format!("read {}", self.root.display()))?
            .is_dir()
        {
            anyhow::bail!("{} is not a directory", self.root.display());
        }

        // The walk is stat-bound, and stats parallelise.
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        let (tx, rx) = mpsc::channel();
        self.builder().threads(threads).build_parallel().run(|| {
            let tx = tx.clone();
            Box::new(move |entry| {
                if let Some(meta) = entry.ok().and_then(|entry| self.file(&entry)) {
                    let _ = tx.send(meta);
                }
                WalkState::Continue
            })
        });
        drop(tx);
        Ok(rx.into_iter().collect())
    }

    /// List and read every indexable file.
    ///
    /// Files that cannot be read (deleted or unreadable since the walk) are
    /// skipped silently.
    pub fn files(&self) -> Result<Vec<SourceFile>> {
        let files = self.meta()?;
        #[cfg(feature = "rayon")]
        {
            use rayon::prelude::*;

            Ok(files.into_par_iter().filter_map(FileMeta::read).collect())
        }
        #[cfg(not(feature = "rayon"))]
        Ok(files.into_iter().filter_map(FileMeta::read).collect())
    }

    /// The walk these limits describe, for a caller that wants the
    /// directories rather than the files.
    pub(crate) fn builder(&self) -> WalkBuilder {
        let mut builder = WalkBuilder::new(self.root);
        builder
            .hidden(!self.opts.hidden)
            .ignore(!self.opts.no_ignore)
            .git_ignore(!self.opts.no_ignore)
            .git_global(!self.opts.no_ignore)
            .git_exclude(!self.opts.no_ignore)
            .parents(!self.opts.no_ignore);
        builder
    }

    fn file(&self, entry: &DirEntry) -> Option<FileMeta> {
        if !entry.file_type()?.is_file() {
            return None;
        }
        let path = self.relative(entry.path())?;
        let stat = entry.metadata().ok()?;
        if !self.opts.covers(&path, stat.len()) {
            return None;
        }
        let mtime_ns = stat
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos() as i64);
        Some(FileMeta {
            path,
            abs: entry.path().to_path_buf(),
            mtime_ns,
            size: stat.len(),
        })
    }

    fn relative(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(self.root).ok()?.to_str()?;
        Some(rel.replace(std::path::MAIN_SEPARATOR, "/"))
    }
}
