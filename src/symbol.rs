use serde::{Deserialize, Serialize};

/// What an identifier occurrence does at its location.
///
/// The discriminants are the on-disk encoding. Their order is also the
/// precedence used when several query patterns match one token: lower wins.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum OccKind {
    /// A definition with a body.
    Definition = 0,
    /// A declaration without a body (trait method, TS `interface` member).
    Declaration = 1,
    /// A call or instantiation.
    Call = 2,
    /// A trait or type named by an `impl` block.
    Impl = 3,
    /// A name introduced by an import.
    Import = 4,
    /// A field or property access.
    Field = 5,
    /// A type mentioned in a type position.
    Type = 6,
    /// Any other qualified path mention.
    Path = 7,
}

impl OccKind {
    const ALL: [OccKind; 8] = [
        OccKind::Definition,
        OccKind::Declaration,
        OccKind::Call,
        OccKind::Impl,
        OccKind::Import,
        OccKind::Field,
        OccKind::Type,
        OccKind::Path,
    ];

    /// How many kinds a posting has to encode.
    pub(crate) const COUNT: usize = Self::ALL.len();

    pub(crate) fn from_byte(byte: u8) -> Self {
        Self::ALL
            .get(byte as usize)
            .copied()
            .unwrap_or(OccKind::Path)
    }

    /// True for occurrences that introduce a symbol.
    pub fn is_definition(self) -> bool {
        matches!(self, OccKind::Definition | OccKind::Declaration)
    }
}

/// The kind of thing a symbol names. The discriminants are the on-disk
/// encoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum SymKind {
    Function = 0,
    Method = 1,
    Class = 2,
    Interface = 3,
    Module = 4,
    Macro = 5,
    Constant = 6,
    Field = 7,
    Variable = 8,
    Type = 9,
    Unknown = 10,
}

impl SymKind {
    /// Every kind, in discriminant order.
    pub const ALL: [SymKind; 11] = [
        SymKind::Function,
        SymKind::Method,
        SymKind::Class,
        SymKind::Interface,
        SymKind::Module,
        SymKind::Macro,
        SymKind::Constant,
        SymKind::Field,
        SymKind::Variable,
        SymKind::Type,
        SymKind::Unknown,
    ];

    /// Lowercase names, in discriminant order: the capture suffix in a tag
    /// query and the word shown for the kind.
    const NAMES: [&str; 11] = [
        "function",
        "method",
        "class",
        "interface",
        "module",
        "macro",
        "constant",
        "field",
        "variable",
        "type",
        "unknown",
    ];

    pub(crate) fn from_byte(byte: u8) -> Self {
        Self::ALL
            .get(byte as usize)
            .copied()
            .unwrap_or(SymKind::Unknown)
    }

    pub(crate) fn from_capture(name: &str) -> Self {
        Self::NAMES
            .iter()
            .position(|known| *known == name)
            .map_or(SymKind::Unknown, |at| Self::ALL[at])
    }

    /// The lowercase name of the kind.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    /// How much the kind says about a symbol. When several query patterns
    /// describe one token (a Go `type_spec` is both a type and a struct, a JS
    /// `const f = () => {}` is both a variable and a function), the most
    /// specific answer wins.
    pub(crate) fn specificity(self) -> u8 {
        match self {
            SymKind::Unknown => 0,
            SymKind::Type => 1,
            SymKind::Variable => 2,
            SymKind::Method => 4,
            _ => 3,
        }
    }

    /// Precedence when one name has several kinds of definition. Lower wins:
    /// a class with a constructor and methods is still a class.
    pub(crate) fn group_rank(self) -> u8 {
        match self {
            SymKind::Class => 0,
            SymKind::Interface => 1,
            SymKind::Module => 2,
            SymKind::Macro => 3,
            SymKind::Function => 4,
            SymKind::Method => 5,
            SymKind::Constant => 6,
            SymKind::Type => 7,
            SymKind::Field => 8,
            SymKind::Variable => 9,
            SymKind::Unknown => 10,
        }
    }
}

/// Separator between a symbol's owner and its name (`ApiError#bad_request`).
///
/// The bare form (`#name`) collects every same-named thing in the tree; the
/// owner-qualified form narrows it to one type.
pub const OWNER_SEP: char = '#';

/// A symbol: a name, and the owning type that narrows it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Symbol<'a> {
    /// The owning type, or `None` for the bare form.
    pub owner: Option<&'a str>,
    pub name: &'a str,
}

impl<'a> Symbol<'a> {
    /// Split a symbol, accepting the stored form (`Owner#name`, `#name`) and
    /// the typed form (`Owner::name`, `name`).
    pub fn parse(sym: &'a str) -> Symbol<'a> {
        let (owner, name) = sym
            .split_once(OWNER_SEP)
            .or_else(|| sym.rsplit_once("::"))
            .unwrap_or(("", sym));
        Symbol {
            owner: (!owner.is_empty()).then_some(owner),
            name,
        }
    }

    /// The symbol an owner names, bare when `owner` is empty.
    pub fn owned(owner: &'a str, name: &'a str) -> Symbol<'a> {
        Symbol {
            owner: (!owner.is_empty()).then_some(owner),
            name,
        }
    }

    /// The stored form: `Owner#name`, or `#name` when bare.
    pub fn stored(&self) -> String {
        format!("{}{OWNER_SEP}{}", self.owner.unwrap_or(""), self.name)
    }

    /// The human-readable form: `Owner::name`, or `name` when bare.
    pub fn pretty(&self) -> String {
        match self.owner {
            Some(owner) => format!("{owner}::{}", self.name),
            None => self.name.to_string(),
        }
    }
}

/// One identifier occurrence found in one file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    /// 1-based line number.
    pub line: u32,
    /// 0-based character offset within the line.
    pub col: u32,
    /// Length in characters.
    pub len: u32,
    pub kind: OccKind,
    pub sym_kind: SymKind,
    pub name: Box<str>,
    /// Owning types, most specific first. A Rust trait method has two: the
    /// concrete type and the trait.
    pub owners: Vec<Box<str>>,
    /// Enclosing definition path, for display in result lists.
    pub context: Option<Box<str>>,
}

impl Tag {
    /// Symbols this occurrence contributes to, most specific first.
    pub fn symbols(&self) -> Vec<String> {
        self.owners
            .iter()
            .map(|owner| Symbol::owned(owner, &self.name).stored())
            .chain(std::iter::once(Symbol::owned("", &self.name).stored()))
            .collect()
    }
}

/// A clickable region in a rendered file.
#[derive(Clone, Debug, Serialize)]
pub struct Span {
    /// 1-based line number.
    pub line: u32,
    /// 0-based character offset within the line.
    pub start: u32,
    /// Length in characters.
    pub len: u32,
    pub kind: OccKind,
    /// Symbols for this region, most specific first.
    pub syms: Vec<String>,
}

impl From<Tag> for Span {
    fn from(tag: Tag) -> Self {
        Span {
            line: tag.line,
            start: tag.col,
            len: tag.len,
            kind: tag.kind,
            syms: tag.symbols(),
        }
    }
}
