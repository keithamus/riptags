use std::cell::RefCell;
use std::collections::HashMap;

use tree_sitter::{Node, Parser, QueryCursor, StreamingIterator};

use crate::lang::{CaptureRole, Lang, LangId};
use crate::symbol::{OccKind, Span, SymKind, Tag};

thread_local! {
    static PARSERS: RefCell<HashMap<LangId, Parser>> = RefCell::new(HashMap::new());
}

/// Extract symbol occurrences from one file.
///
/// Returns an empty vector for languages that are not indexed.
pub fn tag_source(path: &str, source: &str) -> Vec<Tag> {
    match Tagger::of_path(path, source) {
        Some(tagger) => tagger.tags(),
        None => Vec::new(),
    }
}

/// Extract the clickable regions of one file, ready to serialise.
pub fn file_spans(path: &str, source: &str) -> Vec<Span> {
    tag_source(path, source)
        .into_iter()
        .map(Span::from)
        .collect()
}

/// One file being tagged: its grammar and its source.
struct Tagger<'a> {
    lang: &'static Lang,
    source: &'a str,
}

impl<'a> Tagger<'a> {
    /// A tagger for the language `path` names, or `None` when that language
    /// is not indexed.
    fn of_path(path: &str, source: &'a str) -> Option<Tagger<'a>> {
        Some(Tagger {
            lang: Lang::of_path(path)?,
            source,
        })
    }

    /// The source `node` covers.
    fn text(&self, node: Node<'_>) -> &'a str {
        &self.source[node.byte_range()]
    }

    fn tags(&self) -> Vec<Tag> {
        let lang = self.lang;
        let tree = PARSERS.with(|parsers| {
            let mut parsers = parsers.borrow_mut();
            let parser = parsers.entry(lang.id).or_insert_with(|| {
                let mut parser = Parser::new();
                parser
                    .set_language(&lang.language)
                    .expect("grammar matches the tree-sitter ABI");
                parser
            });
            parser.parse(self.source, None)
        });
        let Some(tree) = tree else { return Vec::new() };

        let mut columns = Columns::default();
        let mut tags: Vec<Tag> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&lang.query, tree.root_node(), self.source.as_bytes());

        while let Some(m) = matches.next() {
            let mut name_node: Option<Node> = None;
            let mut qualifier: Option<Node> = None;
            let mut record: Option<(OccKind, SymKind)> = None;
            for capture in m.captures() {
                match lang.roles.get(capture.index as usize) {
                    Some(CaptureRole::Name) => name_node = Some(capture.node),
                    Some(CaptureRole::Qualifier) => qualifier = Some(capture.node),
                    Some(CaptureRole::Record(kind, sym_kind)) => record = Some((*kind, *sym_kind)),
                    _ => {}
                }
            }
            let (Some(name_node), Some((kind, sym_kind))) = (name_node, record) else {
                continue;
            };
            // A C++ definition may name a path (`void net::Socket::open() {}`):
            // the tag belongs on the last segment, owned by the one before it.
            let (name_node, qualifier) = match innermost_name(name_node) {
                Some((name, scope)) => (name, qualifier.or(scope)),
                None => (name_node, qualifier),
            };
            // A Ruby symbol names the method it generates: `attr_accessor :status`
            // answers to `status`.
            let text = self.text(name_node);
            let name = text.strip_prefix(':').unwrap_or(text);
            if name.is_empty() || name == "_" {
                continue;
            }

            let scopes = self.scopes(name_node);
            let qualified = match qualifier {
                Some(node) => scopes.resolve_qualifier(self.text(node)),
                None => Vec::new(),
            };
            // An explicit path names the owner outright; only an unqualified
            // definition takes its owner from the type that encloses it.
            let owners = if qualified.is_empty() && kind.is_definition() {
                scopes.innermost_type()
            } else {
                &qualified
            };

            // The tag starts after any stripped prefix, and its column counts
            // characters, not the bytes tree-sitter reports.
            let start = name_node.end_byte() - name.len();
            let point = name_node.start_position();
            let line_start = name_node.start_byte() - point.column;
            tags.push(Tag {
                line: point.row as u32 + 1,
                col: columns.at(self.source, line_start, start),
                len: name.chars().count() as u32,
                kind,
                sym_kind,
                name: Box::from(name),
                owners: owners.iter().map(|owner| Box::from(*owner)).collect(),
                context: scopes.context(),
            });
        }

        dedupe(tags)
    }

    /// Walk the ancestors of a tagged identifier, innermost first, collecting
    /// the scopes that contribute an owner or context name.
    ///
    /// The definition that the identifier itself names is skipped, so a
    /// method's own name does not become its own context.
    fn scopes(&self, node: Node<'_>) -> Scopes<'a> {
        let mut scopes = Vec::new();
        let mut current = node.parent();
        while let Some(parent) = current {
            for rule in self.lang.scope_rules(parent, self.source) {
                let mut names: Vec<&str> = Vec::new();
                for field in rule.name_fields {
                    // An empty field means the node names itself (a Zig
                    // `variable_declaration` holds its name as a bare child); a
                    // field the grammar does not declare is looked up as a child
                    // kind instead (an Elixir `call` holds its `arguments`).
                    let named = if field.is_empty() {
                        Some(parent)
                    } else {
                        parent.child_by_field_name(field).or_else(|| {
                            parent
                                .named_children(&mut parent.walk())
                                .find(|child| child.kind() == *field)
                        })
                    };
                    let Some(named) = named else { continue };
                    // A path names every segment, most specific first, so either
                    // end of `namespace net::http` or of a Nix `a.b.enable`
                    // answers.
                    if matches!(
                        named.kind(),
                        "nested_namespace_specifier" | "attrpath" | "namespace_name"
                    ) {
                        if encloses(named, node) {
                            continue;
                        }
                        let start = names.len();
                        names.extend(
                            named
                                .named_children(&mut named.walk())
                                .map(|segment| self.text(segment)),
                        );
                        names[start..].reverse();
                        continue;
                    }
                    if let Some(name_node) = scope_name_node(named)
                        && !encloses(name_node, node)
                    {
                        names.push(self.text(name_node));
                    }
                }
                if !names.is_empty() {
                    scopes.push(Scope {
                        names,
                        type_like: rule.type_like,
                    });
                }
            }
            current = parent.parent();
        }
        Scopes(scopes)
    }
}

/// One enclosing syntactic scope. `names` holds every name the scope
/// contributes, most specific first.
struct Scope<'a> {
    names: Vec<&'a str>,
    type_like: bool,
}

/// The scopes enclosing one tagged identifier, innermost first.
struct Scopes<'a>(Vec<Scope<'a>>);

impl<'a> Scopes<'a> {
    /// The names of the innermost type-like scope.
    fn innermost_type(&self) -> &[&'a str] {
        self.0
            .iter()
            .find(|scope| scope.type_like)
            .map(|scope| scope.names.as_slice())
            .unwrap_or_default()
    }

    /// The enclosing definition shown beside an occurrence, outermost first.
    fn context(&self) -> Option<Box<str>> {
        if self.0.is_empty() {
            return None;
        }
        let path: Vec<&str> = self.0.iter().rev().map(|scope| scope.names[0]).collect();
        Some(Box::from(path.join("::")))
    }

    /// Turn the text before a `::`, `.` or `\` into an owning type name.
    ///
    /// `self`, `Self`, `this` and PHP's `static` resolve to the enclosing
    /// type. Everything else is accepted only when it is an identifier with a
    /// leading uppercase, because a lowercase receiver is a variable and a
    /// call chain (`Flag().Envar("X")`) is an expression: neither tells us
    /// anything about the owner. An uppercase binding is taken for a type,
    /// which is the price of not resolving names: `Logger.info()` files the
    /// call under `Logger`.
    fn resolve_qualifier(&self, text: &'a str) -> Vec<&'a str> {
        let last = text.rsplit([':', '.', '\\']).next().unwrap_or(text).trim();
        let last = last.split('<').next().unwrap_or(last).trim();
        if matches!(last, "self" | "Self" | "this" | "static") {
            return self.innermost_type().to_vec();
        }
        let identifier = last
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$');
        if identifier && last.chars().next().is_some_and(char::is_uppercase) {
            return vec![last];
        }
        Vec::new()
    }
}

/// Does `outer` cover the bytes of `inner`?
fn encloses(outer: Node<'_>, inner: Node<'_>) -> bool {
    outer.start_byte() <= inner.start_byte() && inner.end_byte() <= outer.end_byte()
}

/// Split a C++ path (`net::http::Client::send`) into the node that carries
/// the last segment and the node that names its owner.
fn innermost_name<'tree>(node: Node<'tree>) -> Option<(Node<'tree>, Option<Node<'tree>>)> {
    if node.kind() != "qualified_identifier" {
        return None;
    }
    let mut owner = node.child_by_field_name("scope");
    let mut current = node.child_by_field_name("name")?;
    while let Some(inner) = current.child_by_field_name("name") {
        if current.kind() == "qualified_identifier" {
            owner = current.child_by_field_name("scope");
        }
        current = inner;
    }
    Some((current, owner))
}

/// Resolve a scope's name node, digging through wrappers such as
/// `impl Foo<T>` (a `generic_type`) or a Go receiver (`(s *Server)`) to the
/// bare identifier. A type name wins over a binding.
fn scope_name_node<'tree>(named: Node<'tree>) -> Option<Node<'tree>> {
    // A C declarator nests: `Buffer* Pool::take(Config& c)` wraps the name in
    // a pointer around a function declarator. Follow that spine first, or the
    // dig below walks into the parameter list and answers with a parameter
    // type.
    let mut named = named;
    while let Some(inner) = declarator_child(named) {
        named = inner;
    }
    // A leaf is the name itself, whatever the grammar calls the token: PHP
    // spells it `name`, Ruby `constant`, Haskell `name`.
    if named.kind().ends_with("identifier") || named.child_count() == 0 {
        return Some(named);
    }
    descendant_identifier(named, |kind| kind == "type_identifier")
        .or_else(|| descendant_identifier(named, |kind| kind.ends_with("identifier")))
        .or_else(|| {
            // OCaml spells a name `module_name`/`class_name`, PHP `name`,
            // Ruby `constant`, Elixir `alias`.
            descendant_identifier(named, |kind| {
                kind == "name" || kind == "constant" || kind == "alias" || kind.ends_with("_name")
            })
        })
}

/// The declarator one level in, for the C family's nested declarator spine.
fn declarator_child<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
    if !node.kind().ends_with("declarator") {
        return None;
    }
    // `reference_declarator` holds its declarator as an unnamed field.
    node.child_by_field_name("declarator").or_else(|| {
        node.named_children(&mut node.walk())
            .find(|child| child.kind().ends_with("declarator"))
    })
}

fn descendant_identifier<'tree>(
    node: Node<'tree>,
    matches: fn(&str) -> bool,
) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        let depth = stack.len();
        for child in current.children(&mut cursor) {
            if matches(child.kind()) {
                return Some(child);
            }
            stack.push(child);
        }
        // Descend left to right: the first subtree holds the name, the last
        // holds the arguments.
        stack[depth..].reverse();
    }
    None
}

/// The character column of a byte offset, counted on from where the last
/// answer left off.
///
/// Tags arrive in source order for all but a few patterns, so a run of them
/// on one line counts each stretch of that line once instead of recounting
/// from the start of the line - which is what made a minified file quadratic.
#[derive(Default)]
struct Columns {
    line_start: usize,
    byte: usize,
    col: u32,
}

impl Columns {
    fn at(&mut self, source: &str, line_start: usize, byte: usize) -> u32 {
        if self.line_start != line_start || self.byte > byte {
            (self.line_start, self.byte, self.col) = (line_start, line_start, 0);
        }
        self.col += source[self.byte..byte].chars().count() as u32;
        self.byte = byte;
        self.col
    }
}

/// Collapse occurrences that several query patterns matched at the same token,
/// keeping the most specific record kind and any owner that was found.
fn dedupe(mut tags: Vec<Tag>) -> Vec<Tag> {
    tags.sort_by_key(|tag| (tag.line, tag.col, tag.kind as u8));
    let mut out: Vec<Tag> = Vec::with_capacity(tags.len());
    for tag in tags {
        match out.last_mut() {
            Some(last) if last.line == tag.line && last.col == tag.col => {
                if last.owners.is_empty() {
                    last.owners = tag.owners;
                }
                if tag.sym_kind.specificity() > last.sym_kind.specificity() {
                    last.sym_kind = tag.sym_kind;
                }
            }
            _ => out.push(tag),
        }
    }
    out
}
