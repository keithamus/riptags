#![cfg(all(
    feature = "typescript",
    feature = "python",
    feature = "go",
    feature = "c",
    feature = "cpp",
    feature = "javascript"
))]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const RUST_SOURCE: &str = r#"
pub struct ApiError {
    message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError { message: message.into() }
    }

    fn wrap(self) -> Self {
        Self::bad_request(self.message)
    }
}

use crate::api::{Handler, State as Ctx};

trait Failing {
    type Error;
    const RETRIES: u32;

    fn fail(&self) -> ApiError;
}

impl Failing for Handler {
    fn fail(&self) -> ApiError {
        ApiError::bad_request("nope")
    }
}

fn handler(state: &State) -> ApiError {
    let err = ApiError::bad_request("no");
    state.log(&err);
    err
}
"#;

#[test]
fn rust_definition_is_owned_by_its_impl() {
    let tags = tag_source("src/error.rs", RUST_SOURCE);
    let defs = find(&tags, "bad_request", OccKind::Definition);
    assert_eq!(defs.len(), 1, "one definition of bad_request");
    let def = defs[0];
    assert_eq!(def.owners, vec![Box::from("ApiError")]);
    assert_eq!(def.sym_kind, SymKind::Function);
    assert_eq!(def.symbols(), vec!["ApiError#bad_request", "#bad_request"]);
    assert_eq!(def.line, 7);
}

#[test]
fn rust_qualified_callsites_resolve_to_the_owner() {
    let tags = tag_source("src/error.rs", RUST_SOURCE);
    let calls = find(&tags, "bad_request", OccKind::Call);
    assert_eq!(calls.len(), 3, "three callsites, including the Self:: one");
    for call in &calls {
        assert!(
            call.owners.iter().any(|owner| &**owner == "ApiError"),
            "line {} should resolve to ApiError, got {:?}",
            call.line,
            call.owners
        );
    }
    let contexts: Vec<&str> = calls.iter().filter_map(|c| c.context.as_deref()).collect();
    assert!(contexts.contains(&"ApiError::wrap"), "got {contexts:?}");
    assert!(contexts.contains(&"Handler::fail"), "got {contexts:?}");
    assert!(contexts.contains(&"handler"), "got {contexts:?}");
}

#[test]
fn rust_lowercase_receiver_does_not_invent_an_owner() {
    let tags = tag_source("src/error.rs", RUST_SOURCE);
    let calls = find(&tags, "log", OccKind::Call);
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].owners.is_empty(),
        "`state.log()` must not claim owner `state`"
    );
    assert_eq!(calls[0].symbols(), vec!["#log"]);
}

#[test]
fn rust_trait_impl_method_belongs_to_type_and_trait() {
    let tags = tag_source("src/error.rs", RUST_SOURCE);
    let decls = find(&tags, "fail", OccKind::Declaration);
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0].owners, vec![Box::from("Failing")]);

    let defs = find(&tags, "fail", OccKind::Definition);
    assert_eq!(defs.len(), 1);
    assert_eq!(
        defs[0].owners,
        vec![Box::from("Handler"), Box::from("Failing")],
        "a trait impl method belongs to both the type and the trait"
    );
    assert_eq!(
        defs[0].symbols(),
        vec!["Handler#fail", "Failing#fail", "#fail"],
        "so a call through either reaches it"
    );
}

#[test]
fn rust_import_groups_and_trait_items_are_tagged() {
    let tags = tag_source("src/error.rs", RUST_SOURCE);
    for name in ["Handler", "State", "Ctx"] {
        assert_eq!(
            find(&tags, name, OccKind::Import).len(),
            1,
            "`use crate::api::{{Handler, State as Ctx}}` imports {name}"
        );
    }

    let assoc = find(&tags, "Error", OccKind::Declaration);
    assert_eq!(assoc.len(), 1, "`type Error;` is a trait requirement");
    assert_eq!(assoc[0].sym_kind, SymKind::Type);
    assert_eq!(owners(assoc[0]), vec!["Failing"]);

    let retries = find(&tags, "RETRIES", OccKind::Declaration);
    assert_eq!(retries.len(), 1, "a const without a value is a declaration");
    assert_eq!(retries[0].sym_kind, SymKind::Constant);
    assert!(find(&tags, "RETRIES", OccKind::Definition).is_empty());
}

#[test]
fn rust_type_mentions_are_recorded() {
    let tags = tag_source("src/error.rs", RUST_SOURCE);
    let lines: Vec<u32> = find(&tags, "ApiError", OccKind::Type)
        .iter()
        .map(|tag| tag.line)
        .collect();
    assert_eq!(
        lines,
        vec![8, 22, 26, 31],
        "the struct literal and the three return types, and nothing else"
    );
    let defs = find(&tags, "ApiError", OccKind::Definition);
    assert_eq!(
        defs.len(),
        1,
        "the struct itself is a definition, not a type mention"
    );
    assert_eq!(defs[0].sym_kind, SymKind::Class);
}

/// A column counts characters, not bytes, and counting several of them on one
/// line must not lose the multibyte text they follow.
#[test]
fn columns_are_character_offsets() {
    let source = "fn main() {\n    let s = \"\u{65e5}\u{672c}\u{8a9e}\"; greet(s); shout(s);\n}\n";
    let tags = tag_source("src/main.rs", source);
    let line = source.lines().nth(1).unwrap();
    for name in ["greet", "shout"] {
        let call = find(&tags, name, OccKind::Call)[0];
        let chars: String = line
            .chars()
            .skip(call.col as usize)
            .take(name.len())
            .collect();
        assert_eq!(chars, name);
    }
}

const TS_SOURCE: &str = r#"
export class CodeBrowser extends PfElement {
  private cache = new Map<string, string>();

  async loadBlob(path: string): Promise<void> {
    this.renderLines(path);
  }

  renderLines(path: string): void {
    console.log(path);
  }
}

function boot(): void {
  const browser = new CodeBrowser();
  browser.loadBlob("a.ts");
}
"#;

#[test]
fn typescript_methods_are_owned_by_their_class() {
    let tags = tag_source("frontend/code-browser.ts", TS_SOURCE);
    let defs = find(&tags, "renderLines", OccKind::Definition);
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].owners, vec![Box::from("CodeBrowser")]);
    assert_eq!(defs[0].sym_kind, SymKind::Method);

    let calls = find(&tags, "renderLines", OccKind::Call);
    assert_eq!(calls.len(), 1, "`this.renderLines()` is a callsite");
    assert_eq!(
        calls[0].owners,
        vec![Box::from("CodeBrowser")],
        "`this` resolves to the enclosing class"
    );

    let external = find(&tags, "loadBlob", OccKind::Call);
    assert_eq!(external.len(), 1);
    assert!(
        external[0].owners.is_empty(),
        "`browser.loadBlob()` has a lowercase receiver, so no owner is claimed"
    );
}

#[test]
fn unsupported_languages_yield_nothing() {
    assert!(tag_source("README.md", "# hello\n").is_empty());
    assert!(tag_source("Cargo.lock", "[[package]]\n").is_empty());
}

const PY_SOURCE: &str = r#"
import numpy as np

MAX_RETRIES = 8
transform = lambda x: x + 1

class Db:
    table: str = "rows"

    def __init__(self):
        self.key = None

    def get(self, key):
        return self.get(key)


def boot():
    Db().get("k")
"#;

#[test]
fn python_methods_are_owned_by_their_class() {
    let tags = tag_source("app/db.py", PY_SOURCE);
    let defs = find(&tags, "get", OccKind::Definition);
    assert_eq!(defs.len(), 1);
    assert_eq!(owners(defs[0]), vec!["Db"]);
    assert_eq!(
        defs[0].sym_kind,
        SymKind::Method,
        "a def in a class body is a method"
    );
    assert_eq!(defs[0].context.as_deref(), Some("Db"));

    let calls = find(&tags, "get", OccKind::Call);
    assert_eq!(calls.len(), 2, "`self.get()` and `Db().get()`");
    let self_call = calls
        .iter()
        .find(|call| call.context.as_deref() == Some("Db::get"))
        .expect("the recursive call inside `get`");
    assert_eq!(
        owners(self_call),
        vec!["Db"],
        "`self` resolves to the enclosing class"
    );
    assert_eq!(self_call.context.as_deref(), Some("Db::get"));

    let boot = find(&tags, "boot", OccKind::Definition);
    assert_eq!(boot.len(), 1);
    assert!(boot[0].owners.is_empty(), "a free function has no owner");
}

#[test]
fn python_assignments_and_aliases_are_tagged() {
    let tags = tag_source("app/db.py", PY_SOURCE);

    let key = find(&tags, "key", OccKind::Definition);
    assert_eq!(key.len(), 1, "`self.key = None` declares a field");
    assert_eq!(key[0].sym_kind, SymKind::Field);
    assert_eq!(owners(key[0]), vec!["Db"]);

    let table = find(&tags, "table", OccKind::Definition);
    assert_eq!(table.len(), 1, "an annotated class attribute is a field");
    assert_eq!(owners(table[0]), vec!["Db"]);

    let retries = find(&tags, "MAX_RETRIES", OccKind::Definition);
    assert_eq!(retries.len(), 1);
    assert_eq!(retries[0].sym_kind, SymKind::Constant);

    let transform = find(&tags, "transform", OccKind::Definition);
    assert_eq!(transform.len(), 1);
    assert_eq!(
        transform[0].sym_kind,
        SymKind::Function,
        "a bound lambda is a function, not a plain binding"
    );

    let np = find(&tags, "np", OccKind::Import);
    assert_eq!(np.len(), 1, "`import numpy as np` imports `np`");
}

const GO_SOURCE: &str = r#"
package srv

type Server struct {
	port int
}

func (s *Server) Start() error {
	return s.listen()
}

type Reader interface {
	Read(p []byte) (int, error)
}

type Bytes = []byte

var Default = &Server{}

func boot() {
	srv := &Server{}
	srv.Start()
}
"#;

#[test]
fn go_receiver_becomes_the_owner() {
    let tags = tag_source("srv/server.go", GO_SOURCE);
    let defs = find(&tags, "Start", OccKind::Definition);
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].sym_kind, SymKind::Method);
    assert_eq!(
        owners(defs[0]),
        vec!["Server"],
        "the receiver type owns the method, not the receiver binding"
    );

    let server = find(&tags, "Server", OccKind::Definition);
    assert_eq!(server.len(), 1);
    assert_eq!(server[0].sym_kind, SymKind::Class);

    let port = find(&tags, "port", OccKind::Definition);
    assert_eq!(port.len(), 1);
    assert_eq!(owners(port[0]), vec!["Server"]);

    let calls = find(&tags, "Start", OccKind::Call);
    assert_eq!(calls.len(), 1, "`srv.Start()` is a callsite");
    assert!(
        calls[0].owners.is_empty(),
        "`srv` is a lowercase receiver, so no owner is claimed"
    );

    let listen = find(&tags, "listen", OccKind::Call);
    assert_eq!(listen.len(), 1);
    assert_eq!(
        listen[0].context.as_deref(),
        Some("Server::Start"),
        "a call inside a method names the receiver and the method"
    );

    let read = find(&tags, "Read", OccKind::Declaration);
    assert_eq!(read.len(), 1, "an interface method has no body");
    assert_eq!(owners(read[0]), vec!["Reader"]);

    let bytes = find(&tags, "Bytes", OccKind::Definition);
    assert_eq!(bytes.len(), 1, "`type Bytes = []byte` is an alias");
    assert_eq!(bytes[0].sym_kind, SymKind::Type);

    let default = find(&tags, "Default", OccKind::Definition);
    assert_eq!(default.len(), 1, "a package-level var is a definition");
    assert_eq!(default[0].sym_kind, SymKind::Variable);
}

const C_SOURCE: &str = r#"
#define MAX_POINTS 8

struct point {
	int x;
};

static int add(int a) {
	return helper(a);
}
"#;

#[test]
fn c_structs_own_their_fields() {
    let tags = tag_source("src/point.c", C_SOURCE);
    let point = find(&tags, "point", OccKind::Definition);
    assert_eq!(point.len(), 1);
    assert_eq!(point[0].sym_kind, SymKind::Class);

    let field = find(&tags, "x", OccKind::Definition);
    assert_eq!(field.len(), 1);
    assert_eq!(owners(field[0]), vec!["point"]);

    let add = find(&tags, "add", OccKind::Definition);
    assert_eq!(add.len(), 1);
    assert_eq!(add[0].sym_kind, SymKind::Function);
    assert!(add[0].owners.is_empty());

    let helper = find(&tags, "helper", OccKind::Call);
    assert_eq!(helper.len(), 1);
    assert_eq!(helper[0].context.as_deref(), Some("add"));

    let max = find(&tags, "MAX_POINTS", OccKind::Definition);
    assert_eq!(max.len(), 1);
    assert_eq!(max[0].sym_kind, SymKind::Macro);
}

const CPP_SOURCE: &str = r#"
namespace net {
void init();
}

void Server::start() {
	log();
}
"#;

#[test]
fn cpp_out_of_line_definition_keeps_its_owner() {
    let tags = tag_source("src/server.cpp", CPP_SOURCE);
    let start = find(&tags, "start", OccKind::Definition);
    assert_eq!(start.len(), 1);
    assert_eq!(
        owners(start[0]),
        vec!["Server"],
        "`void Server::start()` has no enclosing class node, so the owner \
         comes from the qualified name"
    );

    let init = find(&tags, "init", OccKind::Declaration);
    assert_eq!(init.len(), 1);
    assert_eq!(
        owners(init[0]),
        vec!["net"],
        "a namespace owns the free functions declared in it"
    );

    let log = find(&tags, "log", OccKind::Call);
    assert_eq!(log.len(), 1);
    assert!(log[0].owners.is_empty());
}

const CPP_POINTER_SOURCE: &str = r#"
class Utils {
 public:
	static Manager* GetManager() { return sManager; }
	virtual Object* WrapObject(Context* cx);
	bool operator==(const Utils& other) const;
	~Utils();
};

Widget& ref_get();

char** argv_dup() { return nullptr; }

int (*fp)(int);

namespace net::http {
void Announce();

Socket* Client::CreateSocket(Status& status) {
	return Create(status);
}
}

void net::http::Client::Shutdown() {}
"#;

#[test]
fn cpp_wrapped_declarators_are_tagged() {
    let tags = tag_source("src/utils.cpp", CPP_POINTER_SOURCE);

    let getter = find(&tags, "GetManager", OccKind::Definition);
    assert_eq!(getter.len(), 1, "a pointer return is still a definition");
    assert_eq!(owners(getter[0]), vec!["Utils"]);

    let wrap = find(&tags, "WrapObject", OccKind::Declaration);
    assert_eq!(wrap.len(), 1);
    assert_eq!(owners(wrap[0]), vec!["Utils"]);

    let equals = find(&tags, "operator==", OccKind::Declaration);
    assert_eq!(equals.len(), 1);
    assert_eq!(owners(equals[0]), vec!["Utils"]);

    let dtor = find(&tags, "~Utils", OccKind::Declaration);
    assert_eq!(dtor.len(), 1);
    assert_eq!(owners(dtor[0]), vec!["Utils"]);

    let reference = find(&tags, "ref_get", OccKind::Declaration);
    assert_eq!(
        reference.len(),
        1,
        "a reference return is still a declaration"
    );

    let dup = find(&tags, "argv_dup", OccKind::Definition);
    assert_eq!(
        dup.len(),
        1,
        "a double pointer return is still a definition"
    );

    assert!(
        find(&tags, "fp", OccKind::Declaration).is_empty(),
        "`int (*fp)(int);` is a variable, not a function declaration"
    );
}

#[test]
fn cpp_qualifier_beats_the_enclosing_namespace() {
    let tags = tag_source("src/utils.cpp", CPP_POINTER_SOURCE);

    let create = find(&tags, "CreateSocket", OccKind::Definition);
    assert_eq!(create.len(), 1);
    assert_eq!(
        owners(create[0]),
        vec!["Client"],
        "the class in the qualified name owns the definition, not the namespace"
    );

    let shutdown = find(&tags, "Shutdown", OccKind::Definition);
    assert_eq!(shutdown.len(), 1);
    assert_eq!(owners(shutdown[0]), vec!["Client"]);

    let announce = find(&tags, "Announce", OccKind::Declaration);
    assert_eq!(announce.len(), 1);
    assert_eq!(
        owners(announce[0]),
        vec!["http", "net"],
        "a nested namespace contributes every segment"
    );
}

#[test]
fn cpp_context_names_the_enclosing_function() {
    let tags = tag_source("src/utils.cpp", CPP_POINTER_SOURCE);
    let call = find(&tags, "Create", OccKind::Call);
    assert_eq!(call.len(), 1);
    assert_eq!(
        call[0].context.as_deref(),
        Some("http::Client::CreateSocket"),
        "the context comes from the declarator, never from a parameter type"
    );
}

const JS_SOURCE: &str = r#"
export const Tracker = {
  getTopWindow() {
    return null;
  },
  helper: function () {},
  arrow: () => {},
};

export function focusTop() {
  let win = Tracker.getTopWindow();
  return win;
}
"#;

#[test]
fn javascript_object_literal_methods_are_owned_by_their_binding() {
    let tags = tag_source("src/modules/Tracker.mjs", JS_SOURCE);
    for name in ["getTopWindow", "helper", "arrow"] {
        let defs = find(&tags, name, OccKind::Definition);
        assert_eq!(defs.len(), 1, "one definition of {name}");
        assert_eq!(defs[0].sym_kind, SymKind::Method);
        assert_eq!(owners(defs[0]), vec!["Tracker"]);
    }
}

#[test]
fn javascript_context_stops_at_the_enclosing_function() {
    let tags = tag_source("src/modules/Tracker.mjs", JS_SOURCE);
    let call = find(&tags, "getTopWindow", OccKind::Call);
    assert_eq!(call.len(), 1);
    assert_eq!(
        call[0].context.as_deref(),
        Some("focusTop"),
        "`let win = ...` binds a call, so it names no scope of its own"
    );
}
