use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use tree_sitter::{Language, Query};

use crate::symbol::{OccKind, SymKind};

/// One row per language: the cargo feature that includes it, its identifier,
/// file extensions, grammar, tag query (a file in `queries/`) and scope
/// rules. Row order is `LangId` order, so a `LangId` indexes `SPECS`
/// directly; a language left out of the build leaves out its row as well.
macro_rules! languages {
    ($($feature:literal $id:ident [$($ext:literal),*] $grammar:path, $query:literal, $scopes:expr;)*) => {
        /// A supported language.
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
        pub enum LangId { $(#[cfg(feature = $feature)] $id,)* }

        impl LangId {
            /// Every supported language, in identifier order.
            pub const ALL: &[LangId] = &[$(#[cfg(feature = $feature)] LangId::$id,)*];

            /// Every supported language named, in `LangId` order.
            const NAMES: &'static [&'static str] = &[$(#[cfg(feature = $feature)] $feature,)*];

            /// The file extensions of every language, in `LangId` order.
            const EXTENSIONS: &'static [&'static [&'static str]] =
                &[$(#[cfg(feature = $feature)] &[$($ext),*],)*];

            /// The file extensions this language answers for, without their dot.
            pub fn extensions(self) -> &'static [&'static str] {
                Self::EXTENSIONS[self as usize]
            }

            /// The lowercase name of the language.
            pub fn name(self) -> &'static str {
                Self::NAMES[self as usize]
            }

            /// The language a name or a file extension names, in any case:
            /// `typescript` and `ts` both name TypeScript.
            pub fn of_name(name: &str) -> Option<LangId> {
                let name = name.to_ascii_lowercase();
                Self::NAMES
                    .iter()
                    .position(|known| *known == name)
                    .map(|slot| LangId::ALL[slot])
                    .or_else(|| LangId::of_extension(&name))
            }

            /// The language a file extension names.
            pub fn of_extension(ext: &str) -> Option<LangId> {
                match ext {
                    $(#[cfg(feature = $feature)] $($ext)|* => Some(LangId::$id),)*
                    _ => None,
                }
            }
        }

        const SPECS: &[LangSpec] = &[$(#[cfg(feature = $feature)] LangSpec {
            grammar: || $grammar.into(),
            query: include_str!(concat!("../queries/", $query, ".scm")),
            scopes: $scopes,
        },)*];
    };
}

languages! {
    "rust"       Rust       ["rs"]                                       tree_sitter_rust::LANGUAGE,                  "rust",       scopes::RUST_SCOPES;
    "typescript" TypeScript ["ts", "mts", "cts"]                         tree_sitter_typescript::LANGUAGE_TYPESCRIPT, "typescript", scopes::TS_SCOPES;
    "tsx"        Tsx        ["tsx"]                                      tree_sitter_typescript::LANGUAGE_TSX,        "typescript", scopes::TS_SCOPES;
    "javascript" JavaScript ["js", "mjs", "cjs", "jsx"]                  tree_sitter_javascript::LANGUAGE,            "javascript", scopes::TS_SCOPES;
    "python"     Python     ["py", "pyi"]                                tree_sitter_python::LANGUAGE,                "python",     scopes::PY_SCOPES;
    "go"         Go         ["go"]                                       tree_sitter_go::LANGUAGE,                    "go",         scopes::GO_SCOPES;
    "c"          C          ["c"]                                        tree_sitter_c::LANGUAGE,                     "c",          scopes::C_SCOPES;
    "cpp"        Cpp        ["h", "cc", "cpp", "cxx", "hh", "hpp", "hxx"] tree_sitter_cpp::LANGUAGE,                  "cpp",        scopes::CPP_SCOPES;
    "java"       Java       ["java"]                                     tree_sitter_java::LANGUAGE,                  "java",       scopes::JAVA_SCOPES;
    "csharp"     CSharp     ["cs"]                                       tree_sitter_c_sharp::LANGUAGE,               "csharp",     scopes::CSHARP_SCOPES;
    "kotlin"     Kotlin     ["kt", "kts"]                                tree_sitter_kotlin_ng::LANGUAGE,             "kotlin",     scopes::KOTLIN_SCOPES;
    "scala"      Scala      ["scala", "sc", "sbt"]                       tree_sitter_scala::LANGUAGE,                 "scala",      scopes::SCALA_SCOPES;
    "swift"      Swift      ["swift"]                                    tree_sitter_swift::LANGUAGE,                 "swift",      scopes::SWIFT_SCOPES;
    "php"        Php        ["php"]                                      tree_sitter_php::LANGUAGE_PHP,               "php",        scopes::PHP_SCOPES;
    "ruby"       Ruby       ["rb", "rake", "gemspec"]                    tree_sitter_ruby::LANGUAGE,                  "ruby",       scopes::RUBY_SCOPES;
    "lua"        Lua        ["lua"]                                      tree_sitter_lua::LANGUAGE,                   "lua",        scopes::LUA_SCOPES;
    "bash"       Bash       ["sh", "bash"]                               tree_sitter_bash::LANGUAGE,                  "bash",       scopes::BASH_SCOPES;
    "elixir"     Elixir     ["ex", "exs"]                                tree_sitter_elixir::LANGUAGE,                "elixir",     scopes::ELIXIR_SCOPES;
    "haskell"    Haskell    ["hs"]                                       tree_sitter_haskell::LANGUAGE,               "haskell",    scopes::HASKELL_SCOPES;
    "ocaml"      OCaml      ["ml"]                                       tree_sitter_ocaml::LANGUAGE_OCAML,           "ocaml",      scopes::OCAML_SCOPES;
    "nix"        Nix        ["nix"]                                      tree_sitter_nix::LANGUAGE,                   "nix",        scopes::NIX_SCOPES;
    "zig"        Zig        ["zig"]                                      tree_sitter_zig::LANGUAGE,                   "zig",        scopes::ZIG_SCOPES;
    "html"       Html       ["html", "htm"]                              tree_sitter_html::LANGUAGE,                  "html",       scopes::NO_SCOPES;
    "css"        Css        ["css"]                                      tree_sitter_css::LANGUAGE,                   "css",        scopes::NO_SCOPES;
}

/// What a query capture means to the tagger.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum CaptureRole {
    /// The identifier token itself.
    Name,
    /// The owning expression or path at a use site.
    Qualifier,
    /// The record kind for the whole match.
    Record(OccKind, SymKind),
    Ignored,
}

/// A syntactic scope that contributes an owner or context name.
#[derive(Hash)]
pub(crate) struct ScopeRule {
    /// Node kind that opens the scope.
    pub node: &'static str,
    /// Fields holding the scope's names. A Rust `impl Trait for Type` block
    /// contributes both, so a trait method definition is reachable from the
    /// concrete type *and* from the trait.
    pub name_fields: &'static [&'static str],
    /// True when the scope names a type (and so can own methods and fields).
    pub type_like: bool,
    /// Kinds the scope must hold to open at all. Empty opens always: a
    /// binding scopes only what it binds, so `const T = { m() {} }` owns `m`
    /// while `let win = top()` names nothing.
    pub holds: &'static [&'static str],
    /// Call targets that open the scope. Elixir spells every declaration as
    /// a call, so only the text of the target says what a node declares.
    pub forms: &'static [&'static str],
}

impl ScopeRule {
    /// A scope that owns what it contains: a class, struct, trait or namespace.
    const fn owner(node: &'static str, name_fields: &'static [&'static str]) -> Self {
        ScopeRule {
            node,
            name_fields,
            type_like: true,
            holds: &[],
            forms: &[],
        }
    }

    /// A scope that only names the enclosing context, such as a function.
    const fn context(node: &'static str, name_fields: &'static [&'static str]) -> Self {
        ScopeRule {
            type_like: false,
            ..Self::owner(node, name_fields)
        }
    }

    /// Narrow a scope to the nodes that hold one of `holds`: a JavaScript
    /// binding owns the object it binds, not the call it assigns.
    const fn holding(self, holds: &'static [&'static str]) -> Self {
        ScopeRule { holds, ..self }
    }

    /// Narrow a scope to the calls whose target reads as one of `forms`:
    /// `defmodule Store do` opens a scope, `Store.fetch(key)` does not.
    const fn calling(self, forms: &'static [&'static str]) -> Self {
        ScopeRule { forms, ..self }
    }

    /// Does this scope open at `node`?
    fn opens(&self, node: tree_sitter::Node<'_>, source: &str) -> bool {
        let holds = self.holds.is_empty()
            || node
                .named_children(&mut node.walk())
                .any(|child| self.holds.contains(&child.kind()));
        let calls = self.forms.is_empty()
            || node
                .child_by_field_name("target")
                .is_some_and(|target| self.forms.contains(&&source[target.byte_range()]));
        holds && calls
    }
}

/// A compiled language: its tag query plus the scope rules used to derive
/// owners and enclosing context.
pub struct Lang {
    pub id: LangId,
    pub(crate) language: Language,
    pub(crate) query: Query,
    pub(crate) roles: Vec<CaptureRole>,
    pub(crate) scopes: &'static [ScopeRule],
}

impl Lang {
    /// The compiled language a file path names, or `None` when that language
    /// is not indexed.
    pub fn of_path(path: &str) -> Option<&'static Lang> {
        LangId::of_path(path).map(LangId::compiled)
    }

    /// Every rule that opens a scope at `node`, in table order.
    pub(crate) fn scope_rules<'a>(
        &'a self,
        node: tree_sitter::Node<'a>,
        source: &'a str,
    ) -> impl Iterator<Item = &'a ScopeRule> {
        self.scopes
            .iter()
            .filter(move |rule| rule.node == node.kind() && rule.opens(node, source))
    }
}

/// Scope tables, one per language: a build that leaves a language out
/// leaves its rules unused.
mod scopes {
    #![allow(dead_code)]

    use super::ScopeRule;

    const NAME: &[&str] = &["name"];

    /// A scope that carries its own name as a bare child rather than in a field.
    const SELF: &[&str] = &[""];

    /// Languages whose owners come from qualified names alone.
    pub(super) const NO_SCOPES: &[ScopeRule] = &[];

    pub(super) const RUST_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("impl_item", &["type", "trait"]),
        ScopeRule::owner("trait_item", NAME),
        ScopeRule::owner("struct_item", NAME),
        ScopeRule::owner("enum_item", NAME),
        ScopeRule::owner("enum_variant", NAME),
        ScopeRule::owner("union_item", NAME),
        ScopeRule::context("mod_item", NAME),
        ScopeRule::context("function_item", NAME),
    ];

    pub(super) const TS_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_declaration", NAME),
        ScopeRule::owner("abstract_class_declaration", NAME),
        ScopeRule::owner("class", NAME),
        ScopeRule::owner("interface_declaration", NAME),
        ScopeRule::owner("enum_declaration", NAME),
        ScopeRule::owner("variable_declarator", NAME).holding(&[
            "object",
            "function_expression",
            "arrow_function",
        ]),
        ScopeRule::context("internal_module", NAME),
        ScopeRule::context("function_declaration", NAME),
        ScopeRule::context("method_definition", NAME),
    ];

    pub(super) const PY_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_definition", NAME),
        ScopeRule::context("function_definition", NAME),
    ];

    pub(super) const GO_SCOPES: &[ScopeRule] = &[
        // The context rule comes first so a call inside a method reads
        // `Server::Start`; the receiver still owns the method itself.
        ScopeRule::context("method_declaration", NAME),
        ScopeRule::owner("method_declaration", &["receiver"]),
        ScopeRule::owner("type_spec", NAME),
        ScopeRule::context("function_declaration", NAME),
    ];

    pub(super) const C_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("struct_specifier", NAME),
        ScopeRule::owner("union_specifier", NAME),
        ScopeRule::owner("enum_specifier", NAME),
        ScopeRule::context("function_definition", &["declarator"]),
    ];

    pub(super) const CPP_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_specifier", NAME),
        ScopeRule::owner("struct_specifier", NAME),
        ScopeRule::owner("union_specifier", NAME),
        ScopeRule::owner("enum_specifier", NAME),
        ScopeRule::owner("namespace_definition", NAME),
        ScopeRule::context("function_definition", &["declarator"]),
    ];

    pub(super) const JAVA_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_declaration", NAME),
        ScopeRule::owner("interface_declaration", NAME),
        ScopeRule::owner("enum_declaration", NAME),
        ScopeRule::owner("record_declaration", NAME),
        ScopeRule::owner("annotation_type_declaration", NAME),
        ScopeRule::context("method_declaration", NAME),
        ScopeRule::context("constructor_declaration", NAME),
    ];

    pub(super) const CSHARP_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_declaration", NAME),
        ScopeRule::owner("struct_declaration", NAME),
        ScopeRule::owner("interface_declaration", NAME),
        ScopeRule::owner("record_declaration", NAME),
        ScopeRule::owner("record_struct_declaration", NAME),
        ScopeRule::owner("enum_declaration", NAME),
        ScopeRule::context("namespace_declaration", NAME),
        ScopeRule::context("file_scoped_namespace_declaration", NAME),
        ScopeRule::context("method_declaration", NAME),
        ScopeRule::context("constructor_declaration", NAME),
        ScopeRule::context("local_function_statement", NAME),
    ];

    pub(super) const KOTLIN_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_declaration", NAME),
        ScopeRule::owner("object_declaration", NAME),
        ScopeRule::context("function_declaration", NAME),
    ];

    pub(super) const SCALA_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_definition", NAME),
        ScopeRule::owner("object_definition", NAME),
        ScopeRule::owner("package_object", NAME),
        ScopeRule::owner("given_definition", NAME),
        ScopeRule::owner("trait_definition", NAME),
        ScopeRule::owner("enum_definition", NAME),
        ScopeRule::context("package_clause", NAME),
        ScopeRule::context("function_definition", NAME),
    ];

    pub(super) const SWIFT_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_declaration", NAME),
        ScopeRule::owner("protocol_declaration", NAME),
        ScopeRule::context("function_declaration", NAME),
        // Only a computed property has a body to be the context of.
        ScopeRule::context("property_declaration", NAME).holding(&["computed_property"]),
    ];

    pub(super) const PHP_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class_declaration", NAME),
        ScopeRule::owner("interface_declaration", NAME),
        ScopeRule::owner("trait_declaration", NAME),
        ScopeRule::owner("enum_declaration", NAME),
        ScopeRule::context("namespace_definition", NAME),
        ScopeRule::context("method_declaration", NAME),
        ScopeRule::context("function_definition", NAME),
    ];

    pub(super) const RUBY_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class", NAME),
        ScopeRule::owner("module", NAME),
        ScopeRule::owner("singleton_class", &["value"]),
        ScopeRule::context("method", NAME),
        ScopeRule::context("singleton_method", NAME),
    ];

    /// A Nix attrset is not a type, so a binding only names the enclosing
    /// context: the attrs of `built = pkgs.stdenv.mkDerivation { ... }` show up
    /// as `built::pname` rather than claiming `built` owns them.
    pub(super) const NIX_SCOPES: &[ScopeRule] = &[ScopeRule::context("binding", &["attrpath"])];

    pub(super) const LUA_SCOPES: &[ScopeRule] = &[ScopeRule::context("function_declaration", NAME)];

    pub(super) const BASH_SCOPES: &[ScopeRule] = &[ScopeRule::context("function_definition", NAME)];

    pub(super) const HASKELL_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("class", NAME),
        ScopeRule::owner("instance", &["name", "patterns"]),
        ScopeRule::owner("data_type", NAME),
        ScopeRule::owner("newtype", NAME),
        ScopeRule::context("function", NAME),
    ];

    /// A Zig container is an anonymous value, so the binding that holds it is the
    /// scope: `const Server = struct { ... }` makes `Server` the owner of every
    /// member, however deeply the containers nest.
    pub(super) const ZIG_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("variable_declaration", SELF).holding(&[
            "struct_declaration",
            "union_declaration",
            "enum_declaration",
            "opaque_declaration",
            "error_set_declaration",
        ]),
        ScopeRule::context("function_declaration", NAME),
    ];

    /// OCaml names a module or class with a plain child rather than a field, so
    /// those scopes name themselves. Only a `let` that takes parameters is a
    /// function: `let c = make ()` binds a value and names nothing.
    pub(super) const OCAML_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("module_binding", SELF),
        ScopeRule::owner("class_binding", SELF),
        ScopeRule::owner("type_binding", NAME),
        ScopeRule::context("let_binding", &["pattern"]).holding(&[
            "parameter",
            "fun_expression",
            "function_expression",
        ]),
    ];

    /// Elixir spells every declaration as a call, so a scope is recognised by the
    /// target it calls: `defmodule Store.Cache do` owns the functions defined in
    /// its body, and `def fetch(key) do` names their context.
    pub(super) const ELIXIR_SCOPES: &[ScopeRule] = &[
        ScopeRule::owner("call", &["arguments"]).calling(&["defmodule", "defprotocol", "defimpl"]),
        ScopeRule::context("call", &["arguments"]).calling(&[
            "def",
            "defp",
            "defmacro",
            "defmacrop",
            "defguard",
            "defguardp",
            "defdelegate",
            "defn",
            "defnp",
        ]),
    ];
}

struct LangSpec {
    grammar: fn() -> Language,
    query: &'static str,
    scopes: &'static [ScopeRule],
}

/// Fold everything that decides what a file tags into `hasher`, so an index
/// written before a query or scope edit is discarded rather than trusted.
pub(crate) fn tagging_stamp(hasher: &mut impl Hasher) {
    for spec in SPECS {
        spec.query.hash(hasher);
        spec.scopes.hash(hasher);
    }
}

/// What a capture name means: `@name`, `@qualifier`, or a record such as
/// `@definition.method` or `@reference.call`.
fn role(capture: &str) -> CaptureRole {
    let Some((group, kind)) = capture.split_once('.') else {
        return match capture {
            "name" => CaptureRole::Name,
            "qualifier" => CaptureRole::Qualifier,
            _ => CaptureRole::Ignored,
        };
    };
    let occ = match (group, kind) {
        ("definition", _) => OccKind::Definition,
        ("declaration", _) => OccKind::Declaration,
        ("reference", "call") => OccKind::Call,
        ("reference", "impl") => OccKind::Impl,
        ("reference", "import") => OccKind::Import,
        ("reference", "field") => OccKind::Field,
        ("reference", "type") => OccKind::Type,
        ("reference", _) => OccKind::Path,
        _ => return CaptureRole::Ignored,
    };
    let sym = match occ {
        OccKind::Definition | OccKind::Declaration | OccKind::Type => SymKind::from_capture(kind),
        _ => SymKind::Unknown,
    };
    CaptureRole::Record(occ, sym)
}

fn compile(id: LangId) -> Result<Lang, tree_sitter::QueryError> {
    let spec = &SPECS[id as usize];
    let language = (spec.grammar)();
    let query = Query::new(&language, spec.query)?;
    let roles: Vec<CaptureRole> = query
        .capture_names()
        .iter()
        .map(|name| role(name))
        .collect();
    assert!(
        roles.contains(&CaptureRole::Name),
        "{id:?} tag query must capture @name"
    );
    Ok(Lang {
        id,
        language,
        query,
        roles,
        scopes: spec.scopes,
    })
}

/// Compiled on first use, one slot per language: a query costs milliseconds
/// to compile, and a run touches one or two languages.
static COMPILED: [OnceLock<Lang>; SPECS.len()] = [const { OnceLock::new() }; SPECS.len()];

impl LangId {
    /// The language a file path names, without compiling its query.
    ///
    /// This is the filter the tree walk uses: most files never reach the
    /// tagger.
    pub fn of_path(path: &str) -> Option<LangId> {
        let file = path.rsplit('/').next()?;
        let ext = file.rsplit_once('.')?.1;
        LangId::of_extension(ext)
    }

    /// Is the file at `path` written in one of `langs`? Naming no language
    /// admits every one of them.
    pub fn admits(langs: &[LangId], path: &str) -> bool {
        langs.is_empty() || LangId::of_path(path).is_some_and(|lang| langs.contains(&lang))
    }

    /// The compiled language, compiling its query on first use.
    pub fn compiled(self) -> &'static Lang {
        COMPILED[self as usize].get_or_init(|| {
            compile(self).unwrap_or_else(|e| panic!("invalid {self:?} tag query: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_compiles_and_parses() {
        for id in LangId::ALL {
            let lang = compile(*id).unwrap_or_else(|e| panic!("{id:?} tag query: {e}"));
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&lang.language)
                .unwrap_or_else(|e| panic!("{id:?} grammar rejected by core: {e}"));
            assert!(
                parser.parse("", None).is_some(),
                "{id:?} failed to parse an empty source"
            );
        }
    }

    #[test]
    #[cfg(feature = "default")]
    fn extensions_map_to_languages() {
        let id = |path| Lang::of_path(path).map(|lang| lang.id);
        assert_eq!(id("src/main.rs"), Some(LangId::Rust));
        assert_eq!(id("a/b.tsx"), Some(LangId::Tsx));
        assert_eq!(id("a/b.d.ts"), Some(LangId::TypeScript));
        assert_eq!(id("a/b.py"), Some(LangId::Python));
        assert_eq!(id("a/b.go"), Some(LangId::Go));
        assert_eq!(id("a/b.c"), Some(LangId::C));
        assert_eq!(id("a/b.h"), Some(LangId::Cpp));
        assert_eq!(id("a/B.java"), Some(LangId::Java));
        assert_eq!(id("a/b.kt"), Some(LangId::Kotlin));
        assert_eq!(id("a/b.rb"), Some(LangId::Ruby));
        assert_eq!(id("a/b.html"), Some(LangId::Html));
        assert_eq!(id("a/b.css"), Some(LangId::Css));
        assert!(id("README.md").is_none());
        assert!(id("Makefile").is_none());
    }

    /// A build without a language does not claim its files.
    #[test]
    #[cfg(not(feature = "go"))]
    fn a_language_left_out_is_not_indexed() {
        assert!(LangId::of_path("a/b.go").is_none());
    }
}
