//! The on-disk tag index: fixed-width records plus one string table, read
//! straight out of a memory map.
//!
//! Nothing is deserialised. A query walks the records in place and hands out
//! `&str` slices that point into the mapped file, so opening an index of any
//! size costs one `mmap` call.
//!
//! A sorted symbol table sits beside the tags: one record per `owner#name`
//! holding the slot of every tag that mentions it. A lookup is a binary
//! search over that table plus a walk of one posting list, so it costs what
//! the answer costs, not what the tree costs.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use memmap2::Mmap;

use crate::symbol::{OccKind, SymKind, Tag};

const MAGIC: u32 = 0x5347_5452;
/// Layout revision. Bump when a section changes shape.
const LAYOUT: u32 = 1;

const HEADER: usize = 44;
const FILE_REC: usize = 32;
const TAG_REC: usize = 36;
const OWNER_REC: usize = 8;
const SYM_REC: usize = 28;
const POST_REC: usize = 4;

/// A posting carries the occurrence kind beside the tag slot, so counting the
/// definitions of a symbol never touches a tag record.
// minimal: the three kind bits cap an index at 2^29 tags; tags past that get
// no postings, so a bigger tree needs a wider entry.
const KIND_BITS: u32 = 3;
const KIND_MASK: u32 = (1 << KIND_BITS) - 1;
const MAX_TAGS: usize = 1 << (32 - KIND_BITS);
const _: () = assert!(
    OccKind::COUNT <= 1 << KIND_BITS,
    "a new OccKind needs another kind bit, and the low bits of a posting are the tag slot"
);

/// Stands in for the owner of a bare symbol while the table is built: no
/// string ever lands at this offset.
const NO_OWNER: u32 = u32::MAX;

/// Build stamp: an index written by another build, or before a query or
/// scope-rule edit, holds tags this build would not produce, so it is
/// discarded rather than trusted.
fn stamp() -> u64 {
    let mut hasher = DefaultHasher::default();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    LAYOUT.hash(&mut hasher);
    crate::lang::tagging_stamp(&mut hasher);
    hasher.finish()
}

/// A tag's owning types, whether they came from the map or from a fresh parse.
#[derive(Clone, Copy, Debug)]
enum Owners<'a> {
    Packed {
        records: &'a [u8],
        strings: &'a [u8],
    },
    Parsed(&'a [Box<str>]),
}

/// Iterator over a tag's owners, most specific first.
pub enum OwnersIter<'a> {
    Packed {
        records: std::slice::Iter<'a, [u8; OWNER_REC]>,
        strings: &'a [u8],
    },
    Parsed(std::slice::Iter<'a, Box<str>>),
}

impl<'a> Iterator for OwnersIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self {
            OwnersIter::Packed { records, strings } => records
                .next()
                .map(|owner| text(strings, u32le(owner, 0), u32le(owner, 4))),
            OwnersIter::Parsed(owners) => owners.next().map(|owner| &**owner),
        }
    }
}

/// One identifier occurrence, borrowed from the map or from a parsed `Tag`.
#[derive(Clone, Copy, Debug)]
pub struct TagView<'a> {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub kind: OccKind,
    pub sym_kind: SymKind,
    pub name: &'a str,
    pub context: Option<&'a str>,
    owners: Owners<'a>,
}

impl<'a> TagView<'a> {
    /// View a freshly parsed tag.
    pub fn of(tag: &'a Tag) -> TagView<'a> {
        TagView {
            line: tag.line,
            col: tag.col,
            len: tag.len,
            kind: tag.kind,
            sym_kind: tag.sym_kind,
            name: &tag.name,
            context: tag.context.as_deref(),
            owners: Owners::Parsed(&tag.owners),
        }
    }

    pub fn owners(&self) -> OwnersIter<'a> {
        match self.owners {
            Owners::Packed { records, strings } => OwnersIter::Packed {
                records: records.as_chunks::<OWNER_REC>().0.iter(),
                strings,
            },
            Owners::Parsed(owners) => OwnersIter::Parsed(owners.iter()),
        }
    }

    pub fn owned_by(&self, want: &str) -> bool {
        self.owners().any(|owner| owner == want)
    }
}

/// One indexed file inside the mapped index.
#[derive(Clone, Copy)]
pub struct FileView<'a> {
    index: &'a TagFile,
    record: &'a [u8],
}

impl<'a> FileView<'a> {
    pub fn path(&self) -> &'a str {
        text(
            self.index.strings(),
            u32le(self.record, 0),
            u32le(self.record, 4),
        )
    }

    pub fn mtime_ns(&self) -> i64 {
        u64le(self.record, 16) as i64
    }

    pub fn size(&self) -> u64 {
        u64le(self.record, 24)
    }

    pub fn tag_start(&self) -> u32 {
        u32le(self.record, 8)
    }

    pub fn tag_count(&self) -> usize {
        u32le(self.record, 12) as usize
    }

    pub fn tags(&self) -> impl Iterator<Item = TagView<'a>> + 'a {
        let start = self.tag_start();
        let count = self.tag_count() as u32;
        let index = self.index;
        (start..start + count).filter_map(move |slot| index.tag(slot))
    }
}

/// One symbol of the index: a name, an owner (empty for the bare form that
/// collects every same-named thing), and the slots of its occurrences.
#[derive(Clone, Copy)]
pub struct SymView<'a> {
    index: &'a TagFile,
    record: &'a [u8],
}

impl<'a> SymView<'a> {
    pub fn owner(&self) -> &'a str {
        text(
            self.index.symtext(),
            u32le(self.record, 0),
            u32le(self.record, 4),
        )
    }

    pub fn name(&self) -> &'a str {
        text(
            self.index.symtext(),
            u32le(self.record, 0).saturating_add(u32le(self.record, 4)),
            u32le(self.record, 8),
        )
    }

    /// What this name defines, taking the most telling definition when there
    /// are several: a class with a constructor is a class.
    pub fn kind(&self) -> SymKind {
        SymKind::from_byte(self.record[24])
    }

    /// How many occurrences define or declare the symbol.
    pub fn definitions(&self) -> usize {
        u32le(self.record, 20) as usize
    }

    pub fn occurrences(&self) -> usize {
        u32le(self.record, 16) as usize
    }

    /// Every tag that mentions the symbol, in slot order.
    pub fn slots(&self) -> impl Iterator<Item = (u32, OccKind)> + 'a {
        self.index
            .postings(u32le(self.record, 12) as usize, self.occurrences())
    }
}

/// A memory-mapped tag index.
pub struct TagFile {
    map: Mmap,
    files: usize,
    tags: usize,
    owners: usize,
    syms: usize,
    posts: usize,
    symtext_len: usize,
    strings_len: usize,
}

impl TagFile {
    /// Map the index at `path`, or `None` when it is absent, truncated, or
    /// written by another build.
    pub fn open(path: &Path) -> Option<TagFile> {
        let file = std::fs::File::open(path).ok()?;
        // SAFETY: an index is only ever replaced by rename, never written in
        // place, so the mapped bytes cannot change or shrink under us.
        let map = unsafe { Mmap::map(&file) }.ok()?;
        if map.len() < HEADER
            || u32le(&map, 0) != MAGIC
            || u32le(&map, 4) != LAYOUT
            || u64le(&map, 8) != stamp()
        {
            return None;
        }
        let index = TagFile {
            files: u32le(&map, 16) as usize,
            tags: u32le(&map, 20) as usize,
            owners: u32le(&map, 24) as usize,
            syms: u32le(&map, 28) as usize,
            posts: u32le(&map, 32) as usize,
            symtext_len: u32le(&map, 36) as usize,
            strings_len: u32le(&map, 40) as usize,
            map,
        };
        (index.map.len() >= index.end()).then_some(index)
    }

    fn end(&self) -> usize {
        self.strings_off() + self.strings_len
    }

    fn tags_off(&self) -> usize {
        HEADER + self.files * FILE_REC
    }

    fn owners_off(&self) -> usize {
        self.tags_off() + self.tags * TAG_REC
    }

    fn syms_off(&self) -> usize {
        self.owners_off() + self.owners * OWNER_REC
    }

    fn posts_off(&self) -> usize {
        self.syms_off() + self.syms * SYM_REC
    }

    fn symtext_off(&self) -> usize {
        self.posts_off() + self.posts * POST_REC
    }

    fn strings_off(&self) -> usize {
        self.symtext_off() + self.symtext_len
    }

    fn strings(&self) -> &[u8] {
        &self.map[self.strings_off()..self.end()]
    }

    fn symtext(&self) -> &[u8] {
        &self.map[self.symtext_off()..self.strings_off()]
    }

    pub fn file_count(&self) -> usize {
        self.files
    }

    pub fn symbol_count(&self) -> usize {
        self.syms
    }

    /// The file record in `slot`, which must be below `file_count`.
    fn file_at(&self, slot: usize) -> FileView<'_> {
        let at = HEADER + slot * FILE_REC;
        FileView {
            index: self,
            record: &self.map[at..at + FILE_REC],
        }
    }

    /// Every file, in path order.
    pub fn files(&self) -> impl Iterator<Item = FileView<'_>> + '_ {
        (0..self.files).map(|slot| self.file_at(slot))
    }

    /// The file holding tag `slot`.
    ///
    /// File records hold their tags in one run, and the runs follow the record
    /// order, so this is a binary search.
    pub fn file_of(&self, slot: u32) -> Option<FileView<'_>> {
        let found = bsearch(self.files, |mid| {
            let file = self.file_at(mid);
            let start = file.tag_start();
            if slot < start {
                Ordering::Greater
            } else if slot >= start + file.tag_count() as u32 {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })?;
        Some(self.file_at(found))
    }

    /// The file at `path` with its slot: file records are written in path
    /// order.
    pub fn find_file(&self, path: &str) -> Option<(usize, FileView<'_>)> {
        let found = bsearch(self.files, |mid| self.file_at(mid).path().cmp(path))?;
        Some((found, self.file_at(found)))
    }

    pub fn tag(&self, slot: u32) -> Option<TagView<'_>> {
        let slot = slot as usize;
        if slot >= self.tags {
            return None;
        }
        let at = self.tags_off() + slot * TAG_REC;
        let record = &self.map[at..at + TAG_REC];
        let owner_start = u32le(record, 28) as usize;
        let owner_count = record[34] as usize;
        let owners_at = self.owners_off() + owner_start * OWNER_REC;
        let records = self
            .map
            .get(owners_at..owners_at + owner_count * OWNER_REC)
            .unwrap_or(&[]);
        let strings = self.strings();
        let context_len = u32le(record, 24);
        Some(TagView {
            line: u32le(record, 0),
            col: u32le(record, 4),
            len: u32le(record, 8),
            kind: OccKind::from_byte(record[32]),
            sym_kind: SymKind::from_byte(record[33]),
            name: text(strings, u32le(record, 12), u32le(record, 16)),
            context: (context_len > 0).then(|| text(strings, u32le(record, 20), context_len)),
            owners: Owners::Packed { records, strings },
        })
    }

    /// The symbol `owner#name`, or `#name` when `owner` is empty.
    pub fn symbol(&self, owner: &str, name: &str) -> Option<SymView<'_>> {
        let found = bsearch(self.syms, |mid| {
            let sym = self.sym_at(mid);
            (sym.name(), sym.owner()).cmp(&(name, owner))
        })?;
        Some(self.sym_at(found))
    }

    /// Every symbol, ordered by name and then by owner.
    pub fn symbols(&self) -> impl Iterator<Item = SymView<'_>> + '_ {
        (0..self.syms).map(|slot| self.sym_at(slot))
    }

    /// The symbol record in `slot`, which must be below `symbol_count`.
    fn sym_at(&self, slot: usize) -> SymView<'_> {
        let at = self.syms_off() + slot * SYM_REC;
        SymView {
            index: self,
            record: &self.map[at..at + SYM_REC],
        }
    }

    fn postings(&self, start: usize, count: usize) -> impl Iterator<Item = (u32, OccKind)> + '_ {
        let at = self.posts_off() + start * POST_REC;
        self.map
            .get(at..at + count * POST_REC)
            .unwrap_or(&[])
            .as_chunks::<POST_REC>()
            .0
            .iter()
            .map(|entry| {
                let packed = u32le(entry, 0);
                (
                    packed >> KIND_BITS,
                    OccKind::from_byte((packed & KIND_MASK) as u8),
                )
            })
    }
}

/// Binary search over `len` records ordered by `probe`, which says how the
/// record at an index compares to the wanted one.
fn bsearch(len: usize, probe: impl Fn(usize) -> Ordering) -> Option<usize> {
    let (mut low, mut high) = (0, len);
    while low < high {
        let mid = (low + high) / 2;
        match probe(mid) {
            Ordering::Less => low = mid + 1,
            Ordering::Greater => high = mid,
            Ordering::Equal => return Some(mid),
        }
    }
    None
}

/// Builds the on-disk index: fixed-width file, tag and owner records over one
/// deduplicated string table.
#[derive(Default)]
pub struct TagFileWriter {
    files: Vec<u8>,
    tags: Vec<u8>,
    owners: Vec<u8>,
    strings: Vec<u8>,
    interned: HashMap<Box<str>, (u32, u32)>,
    open_file: Option<(usize, u32)>,
}

impl TagFileWriter {
    /// Start a file's records. Every following `push` belongs to it.
    pub fn file(&mut self, path: &str, mtime_ns: i64, size: u64) {
        self.close_file();
        let (path_off, path_len) = self.intern(path);
        let record = self.files.len();
        let tag_start = (self.tags.len() / TAG_REC) as u32;
        put32(&mut self.files, [path_off, path_len, tag_start, 0]);
        self.files.extend((mtime_ns as u64).to_le_bytes());
        self.files.extend(size.to_le_bytes());
        self.open_file = Some((record, 0));
    }

    /// Append one tag to the file most recently opened.
    pub fn push(&mut self, tag: TagView<'_>) {
        let (name_off, name_len) = self.intern(tag.name);
        let (context_off, context_len) = self.intern(tag.context.unwrap_or(""));
        let owner_start = (self.owners.len() / OWNER_REC) as u32;
        let mut owner_count = 0u8;
        for owner in tag.owners() {
            let (off, len) = self.intern(owner);
            put32(&mut self.owners, [off, len]);
            owner_count = owner_count.saturating_add(1);
        }
        put32(
            &mut self.tags,
            [
                tag.line,
                tag.col,
                tag.len,
                name_off,
                name_len,
                context_off,
                context_len,
                owner_start,
            ],
        );
        self.tags
            .extend([tag.kind as u8, tag.sym_kind as u8, owner_count, 0]);
        if let Some((_, count)) = &mut self.open_file {
            *count += 1;
        }
    }

    /// The finished index bytes, ready to write.
    pub fn finish(mut self) -> Vec<u8> {
        self.close_file();
        let (syms, posts, symtext) = self.symbol_table();
        let mut out = Vec::with_capacity(
            HEADER
                + self.files.len()
                + self.tags.len()
                + self.owners.len()
                + syms.len()
                + posts.len()
                + symtext.len()
                + self.strings.len(),
        );
        put32(&mut out, [MAGIC, LAYOUT]);
        out.extend(stamp().to_le_bytes());
        put32(
            &mut out,
            [
                (self.files.len() / FILE_REC) as u32,
                (self.tags.len() / TAG_REC) as u32,
                (self.owners.len() / OWNER_REC) as u32,
                (syms.len() / SYM_REC) as u32,
                (posts.len() / POST_REC) as u32,
                symtext.len() as u32,
                self.strings.len() as u32,
            ],
        );
        for section in [
            &self.files,
            &self.tags,
            &self.owners,
            &syms,
            &posts,
            &symtext,
            &self.strings,
        ] {
            out.extend_from_slice(section);
        }
        out
    }

    /// Invert the tags into the symbol table: one record per `owner#name`,
    /// ordered by name then owner, each over one run of postings.
    fn symbol_table(&self) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let tags = (self.tags.len() / TAG_REC).min(MAX_TAGS);
        let mut entries: Vec<[u32; 3]> = Vec::with_capacity(tags + self.owners.len() / OWNER_REC);
        for slot in 0..tags {
            let record = &self.tags[slot * TAG_REC..];
            let name = u32le(record, 12);
            let posting = ((slot as u32) << KIND_BITS) | record[32] as u32;
            entries.push([name, NO_OWNER, posting]);
            let owner_start = u32le(record, 28) as usize;
            for owner in 0..record[34] as usize {
                let at = (owner_start + owner) * OWNER_REC;
                // An owner with no text owns nothing: it is the bare symbol,
                // which the entry above already carries.
                let owner = match u32le(&self.owners, at + 4) {
                    0 => NO_OWNER,
                    _ => u32le(&self.owners, at),
                };
                entries.push([name, owner, posting]);
            }
        }
        entries.sort_unstable();
        entries.dedup();

        let mut interned: Vec<(u32, u32)> = self.interned.values().copied().collect();
        interned.sort_unstable();
        let text_of = |off: u32| match interned.binary_search_by_key(&off, |(at, _)| *at) {
            Ok(found) => text(&self.strings, interned[found].0, interned[found].1),
            Err(_) => "",
        };

        // Runs of one symbol in `entries`: (name, owner, start, count).
        let mut runs: Vec<(u32, u32, u32, u32)> = Vec::new();
        for (at, entry) in entries.iter().enumerate() {
            match runs.last_mut() {
                Some(last) if last.0 == entry[0] && last.1 == entry[1] => last.3 += 1,
                _ => runs.push((entry[0], entry[1], at as u32, 1)),
            }
        }
        let mut groups: Vec<(&str, &str, u32, u32)> = runs
            .iter()
            .map(|&(name, owner, start, count)| (text_of(name), text_of(owner), start, count))
            .collect();
        groups.sort_unstable();

        let mut syms = Vec::with_capacity(groups.len() * SYM_REC);
        let mut posts = Vec::with_capacity(entries.len() * POST_REC);
        let mut symtext: Vec<u8> = Vec::new();
        for (name, owner, start, count) in groups {
            let at = symtext.len() as u32;
            symtext.extend_from_slice(owner.as_bytes());
            symtext.extend_from_slice(name.as_bytes());
            let post_start = (posts.len() / POST_REC) as u32;
            let mut definitions = 0u32;
            let mut kind = SymKind::Unknown;
            for entry in &entries[start as usize..(start + count) as usize] {
                posts.extend(entry[2].to_le_bytes());
                if OccKind::from_byte((entry[2] & KIND_MASK) as u8).is_definition() {
                    definitions += 1;
                    let slot = (entry[2] >> KIND_BITS) as usize;
                    let candidate = SymKind::from_byte(self.tags[slot * TAG_REC + 33]);
                    if candidate.group_rank() < kind.group_rank() {
                        kind = candidate;
                    }
                }
            }
            put32(
                &mut syms,
                [
                    at,
                    owner.len() as u32,
                    name.len() as u32,
                    post_start,
                    count,
                    definitions,
                ],
            );
            syms.extend([kind as u8, 0, 0, 0]);
        }
        (syms, posts, symtext)
    }

    fn close_file(&mut self) {
        if let Some((record, count)) = self.open_file.take() {
            self.files[record + 12..record + 16].copy_from_slice(&count.to_le_bytes());
        }
    }

    /// Offsets are the identity of a string here, so the empty string never
    /// enters the table: it would share an offset with whatever comes next.
    fn intern(&mut self, text: &str) -> (u32, u32) {
        if text.is_empty() {
            return (0, 0);
        }
        if let Some(found) = self.interned.get(text) {
            return *found;
        }
        let entry = (self.strings.len() as u32, text.len() as u32);
        self.strings.extend_from_slice(text.as_bytes());
        self.interned.insert(Box::from(text), entry);
        entry
    }
}

fn put32<const N: usize>(out: &mut Vec<u8>, words: [u32; N]) {
    out.extend(words.into_iter().flat_map(u32::to_le_bytes));
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .map_or(0, |slice| u32::from_le_bytes(slice.try_into().unwrap()))
}

fn u64le(bytes: &[u8], at: usize) -> u64 {
    bytes
        .get(at..at + 8)
        .map_or(0, |slice| u64::from_le_bytes(slice.try_into().unwrap()))
}

/// A string-table slice. Anything out of range reads as empty rather than
/// panicking, so a corrupt index degrades instead of crashing.
fn text(strings: &[u8], off: u32, len: u32) -> &str {
    let (off, len) = (off as usize, len as usize);
    strings
        .get(off..off + len)
        .and_then(|slice| std::str::from_utf8(slice).ok())
        .unwrap_or("")
}
