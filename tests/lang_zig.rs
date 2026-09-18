#![cfg(feature = "zig")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const ZIG_SOURCE: &str = r#"
const std = @import("std");
const util = @import("./util.zig");

pub const Server = struct {
    port: u16,
    name: []const u8,

    const default_port: u16 = 8080;
    var live: u32 = 0;

    pub fn init(port: u16) Server {
        return Server{ .port = port, .name = "s" };
    }

    pub fn start(self: *Server) void {
        std.debug.print("{d}\n", .{self.port});
    }
};

pub const Mode = enum { fast, slow };

pub const Error = error{ OutOfRange };

const max_conns: u32 = 16;

pub fn run(server: *Server, mode: Mode) void {
    const local = server;
    local.start();
    _ = Server.init(max_conns);
    _ = mode;
}

test run {
    _ = run;
}

pub const Packed = packed struct { lo: u4, hi: u4 };
pub const Alias = Packed;

pub const Outer = struct {
    pub const Inner = struct { val: u8 };
    inner: Inner,
};

pub const Err = error{ Bad };

fn boom() Err!void {
    return error.Bad;
}
"#;

#[test]
fn zig_definitions_and_references() {
    let tags = tag_source("src/main.zig", ZIG_SOURCE);

    // `const Server = struct { ... }` defines a type, not a constant: the
    // container-bound pattern must win over the plain `const` one.
    let server = find(&tags, "Server", OccKind::Definition);
    assert_eq!(server.len(), 1);
    assert_eq!(server[0].sym_kind, SymKind::Class);
    assert_eq!((server[0].line, server[0].col, server[0].len), (5, 10, 6));
    assert!(
        owners(server[0]).is_empty(),
        "Zig declares no type-like scopes"
    );

    let mode = find(&tags, "Mode", OccKind::Definition);
    assert_eq!(mode.len(), 1);
    assert_eq!(mode[0].sym_kind, SymKind::Class, "an enum is a class");

    let error_set = find(&tags, "Error", OccKind::Definition);
    assert_eq!(error_set.len(), 1);
    assert_eq!(error_set[0].sym_kind, SymKind::Type);

    let out_of_range = find(&tags, "OutOfRange", OccKind::Definition);
    assert_eq!(out_of_range.len(), 1);
    assert_eq!(out_of_range[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(out_of_range[0]), vec!["Error"]);

    // A function inside the container takes the container as owner via
    // `@qualifier`; a free function has none.
    let init = find(&tags, "init", OccKind::Definition);
    assert_eq!(init.len(), 1);
    assert_eq!(init[0].sym_kind, SymKind::Function);
    assert_eq!(owners(init[0]), vec!["Server"]);

    let run_def = find(&tags, "run", OccKind::Definition);
    assert_eq!(run_def.len(), 1);
    assert_eq!(run_def[0].sym_kind, SymKind::Function);
    assert!(owners(run_def[0]).is_empty());

    // Fields and container-level declarations.
    let port = find(&tags, "port", OccKind::Definition);
    assert_eq!(port.len(), 1);
    assert_eq!(port[0].sym_kind, SymKind::Field);
    assert_eq!(owners(port[0]), vec!["Server"]);
    assert_eq!((port[0].line, port[0].col, port[0].len), (6, 4, 4));

    let fast = find(&tags, "fast", OccKind::Definition);
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].sym_kind, SymKind::Field, "an enum tag is a field");
    assert_eq!(owners(fast[0]), vec!["Mode"]);

    let default_port = find(&tags, "default_port", OccKind::Definition);
    assert_eq!(default_port.len(), 1);
    assert_eq!(default_port[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(default_port[0]), vec!["Server"]);

    let live = find(&tags, "live", OccKind::Definition);
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].sym_kind, SymKind::Variable);
    assert_eq!(owners(live[0]), vec!["Server"]);

    let max_conns = find(&tags, "max_conns", OccKind::Definition);
    assert_eq!(max_conns.len(), 1);
    assert_eq!(max_conns[0].sym_kind, SymKind::Constant);
    assert!(owners(max_conns[0]).is_empty());

    // The binding that holds a container names the scope its members sit in.
    assert_eq!(port[0].context.as_deref(), Some("Server"));
    assert_eq!(init[0].context.as_deref(), Some("Server"));
    assert_eq!(
        find(&tags, "print", OccKind::Call)[0].context.as_deref(),
        Some("Server::start"),
        "a call inside a container method names both"
    );
    assert_eq!(
        find(&tags, "init", OccKind::Call)[0].context.as_deref(),
        Some("run"),
        "a free function names its own context"
    );

    // Declarations inside a function body are locals, not searchable symbols.
    assert!(
        find(&tags, "local", OccKind::Definition).is_empty(),
        "`const local = server` in a block is a local"
    );
    assert!(
        find(&tags, "server", OccKind::Definition).is_empty(),
        "parameters are not tagged"
    );

    // `Server.init(max_conns)`: a TitleCase receiver claims the owner.
    let init_call = find(&tags, "init", OccKind::Call);
    assert_eq!(init_call.len(), 1);
    assert_eq!(owners(init_call[0]), vec!["Server"]);
    assert_eq!(init_call[0].line, 30);

    // `local.start()`: a lowercase receiver is indistinguishable from a local,
    // so the call claims no owner.
    let start_call = find(&tags, "start", OccKind::Call);
    assert_eq!(start_call.len(), 1);
    assert!(owners(start_call[0]).is_empty());

    // `std.debug.print(...)`: the callee is a call, `debug` a field read.
    let print_call = find(&tags, "print", OccKind::Call);
    assert_eq!(print_call.len(), 1);
    assert!(owners(print_call[0]).is_empty());
    assert_eq!(find(&tags, "debug", OccKind::Field).len(), 1);

    // `@import("std")` imports a module name; a relative path is not a symbol.
    let import = find(&tags, "std", OccKind::Import);
    assert_eq!(import.len(), 1);
    assert_eq!(import[0].line, 2);
    assert!(
        find(&tags, "./util.zig", OccKind::Import).is_empty(),
        "relative @import paths are not symbols"
    );
    // The binding itself is still a container-level constant.
    assert_eq!(find(&tags, "std", OccKind::Definition).len(), 1);

    // `self.port` and the `Server{ .port = ... }` designator are field reads.
    let port_refs = find(&tags, "port", OccKind::Field);
    assert_eq!(port_refs.len(), 2);
    assert_eq!(
        port_refs
            .iter()
            .filter(|tag| owners(tag) == vec!["Server"])
            .count(),
        1,
        "`self.port` inside a member function reads the container's field"
    );

    // Type mentions: `Server` return type, `Server{...}` initializer, the
    // `*Server` self parameter and the `*Server` parameter of `run`.
    let server_types = find(&tags, "Server", OccKind::Type);
    assert_eq!(server_types.len(), 4);
    assert_eq!(
        server_types.iter().map(|tag| tag.line).collect::<Vec<_>>(),
        vec![12, 13, 16, 27]
    );
    assert_eq!(find(&tags, "Mode", OccKind::Type).len(), 1);

    // `test run { ... }` points back at the declaration under test.
    assert_eq!(find(&tags, "run", OccKind::Path).len(), 1);

    // `packed struct` is still a class; the alias binding is a constant and
    // must not claim a second definition of the aliased type.
    let packed = find(&tags, "Packed", OccKind::Definition);
    assert_eq!(packed.len(), 1, "`const Alias = Packed;` defines nothing");
    assert_eq!(packed[0].sym_kind, SymKind::Class);
    assert_eq!(packed[0].line, 38);
    let alias = find(&tags, "Alias", OccKind::Definition);
    assert_eq!(alias.len(), 1);
    assert_eq!(alias[0].sym_kind, SymKind::Constant);
    let packed_ref = find(&tags, "Packed", OccKind::Type);
    assert_eq!(packed_ref.len(), 1);
    assert_eq!(packed_ref[0].line, 39);

    // A container nested in a container: class, owned by the outer binding.
    let inner = find(&tags, "Inner", OccKind::Definition);
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].sym_kind, SymKind::Class);
    assert_eq!(owners(inner[0]), vec!["Outer"]);
    let val = find(&tags, "val", OccKind::Definition);
    assert_eq!(val.len(), 1);
    assert_eq!(owners(val[0]), vec!["Inner"], "the innermost binding owns");

    // Error sets: the binding is a type, its members constants owned by it,
    // and `error.Bad` is a member read.
    let err = find(&tags, "Err", OccKind::Definition);
    assert_eq!(err.len(), 1);
    assert_eq!(err[0].sym_kind, SymKind::Type);
    let bad = find(&tags, "Bad", OccKind::Definition);
    assert_eq!(bad.len(), 1);
    assert_eq!(bad[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(bad[0]), vec!["Err"]);
    let bad_ref = find(&tags, "Bad", OccKind::Field);
    assert_eq!(bad_ref.len(), 1);
    assert_eq!(bad_ref[0].line, 49);
    assert!(owners(bad_ref[0]).is_empty());
    assert_eq!(find(&tags, "Err", OccKind::Type).len(), 1, "`Err!void`");
}
