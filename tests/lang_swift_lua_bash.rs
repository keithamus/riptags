#![cfg(all(feature = "swift", feature = "lua", feature = "bash"))]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const SWIFT_SOURCE: &str = r#"
import Foundation

var activeMode = Mode.fast
let defaultPort = 80

protocol Service {
    init(config: Config)
    func start() throws
    var label: String { get }
}

struct Config {
    let port: Int
}

final class Server: Service {
    typealias Handler = (Config) -> Void

    static let shared = Server(config: Config(port: defaultPort))
    var config: Config

    init(config: Config) {
        self.config = config
    }

    deinit {}

    func start() throws {
        self.listen()
        Logger.log("up")
    }

    private func listen() {}

    var total: Int {
        get { return audit() }
    }
}

enum Mode {
    case fast
    case slow

    func label() -> String {
        return "mode"
    }
}

extension Server {
    func stop() {}
}

func boot() throws {
    let server = Server(config: Config(port: 8080))
    try server.start()
}
"#;

#[test]
fn swift_types_own_their_members() {
    let tags = tag_source("app/Server.swift", SWIFT_SOURCE);

    let server = find(&tags, "Server", OccKind::Definition);
    assert_eq!(server.len(), 1);
    assert_eq!(server[0].sym_kind, SymKind::Class);
    assert!(server[0].owners.is_empty(), "a top-level type has no owner");
    assert_eq!(server[0].context, None);

    let service = find(&tags, "Service", OccKind::Definition);
    assert_eq!(service.len(), 1);
    assert_eq!(
        service[0].sym_kind,
        SymKind::Interface,
        "a protocol is an interface"
    );

    let config = find(&tags, "Config", OccKind::Definition);
    assert_eq!(config.len(), 1);
    assert_eq!(config[0].sym_kind, SymKind::Class, "a struct is a class");

    let mode = find(&tags, "Mode", OccKind::Definition);
    assert_eq!(mode.len(), 1);
    assert_eq!(mode[0].sym_kind, SymKind::Class, "an enum is a class");

    // A method defined in the class body, the protocol requirement it
    // satisfies, and the callsite that reaches it through `self`.
    let defs = find(&tags, "start", OccKind::Definition);
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].sym_kind, SymKind::Method);
    assert_eq!(owners(defs[0]), vec!["Server"]);
    assert_eq!(defs[0].context.as_deref(), Some("Server"));

    let decls = find(&tags, "start", OccKind::Declaration);
    assert_eq!(decls.len(), 1, "the protocol requirement has no body");
    assert_eq!(decls[0].sym_kind, SymKind::Method);
    assert_eq!(owners(decls[0]), vec!["Service"]);

    let init_defs = find(&tags, "init", OccKind::Definition);
    assert_eq!(init_defs.len(), 1);
    assert_eq!(init_defs[0].sym_kind, SymKind::Method);
    assert_eq!(owners(init_defs[0]), vec!["Server"]);
    let init_decls = find(&tags, "init", OccKind::Declaration);
    assert_eq!(init_decls.len(), 1, "the protocol requirement has no body");
    assert_eq!(owners(init_decls[0]), vec!["Service"]);

    let audit = find(&tags, "audit", OccKind::Call);
    assert_eq!(audit.len(), 1);
    assert_eq!(
        audit[0].context.as_deref(),
        Some("Server::total"),
        "a call in a computed property names the property"
    );

    let stop = find(&tags, "stop", OccKind::Definition);
    assert_eq!(stop.len(), 1);
    assert_eq!(
        owners(stop[0]),
        vec!["Server"],
        "an extension member belongs to the extended type"
    );

    let port = find(&tags, "port", OccKind::Definition);
    assert_eq!(port.len(), 1);
    assert_eq!(port[0].sym_kind, SymKind::Field);
    assert_eq!(owners(port[0]), vec!["Config"]);

    let fast = find(&tags, "fast", OccKind::Definition);
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].sym_kind, SymKind::Field, "an enum case is a field");
    assert_eq!(owners(fast[0]), vec!["Mode"]);

    let handler = find(&tags, "Handler", OccKind::Definition);
    assert_eq!(handler.len(), 1);
    assert_eq!(handler[0].sym_kind, SymKind::Type);
    assert_eq!(owners(handler[0]), vec!["Server"]);

    let boot = find(&tags, "boot", OccKind::Definition);
    assert_eq!(boot.len(), 1);
    assert_eq!(
        boot[0].sym_kind,
        SymKind::Function,
        "a top-level func is a function, not a method"
    );
    assert!(boot[0].owners.is_empty());

    let default_port = find(&tags, "defaultPort", OccKind::Definition);
    assert_eq!(default_port.len(), 1);
    assert_eq!(default_port[0].sym_kind, SymKind::Constant, "top-level let");
    let active_mode = find(&tags, "activeMode", OccKind::Definition);
    assert_eq!(active_mode.len(), 1);
    assert_eq!(active_mode[0].sym_kind, SymKind::Variable, "top-level var");

    // References.
    let listen_calls = find(&tags, "listen", OccKind::Call);
    assert_eq!(listen_calls.len(), 1);
    assert_eq!(
        owners(listen_calls[0]),
        vec!["Server"],
        "`self.listen()` resolves to the enclosing type"
    );
    assert_eq!(
        listen_calls[0].context.as_deref(),
        Some("Server::start"),
        "the callsite is inside `Server.start`"
    );

    let log = find(&tags, "log", OccKind::Call);
    assert_eq!(log.len(), 1);
    assert_eq!(owners(log[0]), vec!["Logger"]);

    let start_calls = find(&tags, "start", OccKind::Call);
    assert_eq!(start_calls.len(), 1, "`server.start()` is a callsite");
    assert!(
        start_calls[0].owners.is_empty(),
        "`server` is a lowercase receiver, so no owner is claimed"
    );

    let conformance = find(&tags, "Service", OccKind::Impl);
    assert_eq!(conformance.len(), 1, "`Server: Service` conforms");
    let extension = find(&tags, "Server", OccKind::Impl);
    assert_eq!(extension.len(), 1, "`extension Server` extends");

    let import = find(&tags, "Foundation", OccKind::Import);
    assert_eq!(import.len(), 1);

    let case_use = find(&tags, "fast", OccKind::Field);
    assert_eq!(case_use.len(), 1, "`Mode.fast` reads a member");
    assert_eq!(owners(case_use[0]), vec!["Mode"]);
}

const LUA_SOURCE: &str = r#"
local http = require("socket.http")

local M = {}

function M.new(host)
  return setmetatable({ host = host }, M)
end

function M:fetch(path)
  return http.request(self.host .. path)
end

M.close = function(self)
  self.host = nil
end

local function normalize(path)
  return path
end

local trimmer = function(s)
  return s
end

local Config = {
  parse = function(text)
    return text
  end,
}

function boot()
  local client = M.new("example.com")
  client:fetch(normalize("/"))
end
"#;

#[test]
fn lua_table_functions_are_owned_by_their_table() {
    let tags = tag_source("lib/client.lua", LUA_SOURCE);

    let new = find(&tags, "new", OccKind::Definition);
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].sym_kind, SymKind::Function);
    assert_eq!(
        owners(new[0]),
        vec!["M"],
        "`function M.new()` belongs to table `M`"
    );
    assert_eq!(new[0].context.as_deref(), Some("M"));

    let fetch = find(&tags, "fetch", OccKind::Definition);
    assert_eq!(fetch.len(), 1);
    assert_eq!(
        fetch[0].sym_kind,
        SymKind::Method,
        "`function M:fetch()` takes an implicit self"
    );
    assert_eq!(owners(fetch[0]), vec!["M"]);

    let close = find(&tags, "close", OccKind::Definition);
    assert_eq!(close.len(), 1, "`M.close = function() end` defines close");
    assert_eq!(close[0].sym_kind, SymKind::Function);
    assert_eq!(owners(close[0]), vec!["M"]);

    let normalize = find(&tags, "normalize", OccKind::Definition);
    assert_eq!(normalize.len(), 1);
    assert!(
        normalize[0].owners.is_empty(),
        "a local function has no owner"
    );
    assert_eq!(normalize[0].context, None);

    let trimmer = find(&tags, "trimmer", OccKind::Definition);
    assert_eq!(trimmer.len(), 1, "`local trimmer = function() end`");
    assert_eq!(trimmer[0].sym_kind, SymKind::Function);

    let parse = find(&tags, "parse", OccKind::Definition);
    assert_eq!(parse.len(), 1);
    assert_eq!(
        owners(parse[0]),
        vec!["Config"],
        "`local Config = {{ parse = function() end }}` belongs to `Config`"
    );

    // References.
    let new_calls = find(&tags, "new", OccKind::Call);
    assert_eq!(new_calls.len(), 1);
    assert_eq!(owners(new_calls[0]), vec!["M"], "`M.new()` names its table");
    assert_eq!(new_calls[0].context.as_deref(), Some("boot"));

    let fetch_calls = find(&tags, "fetch", OccKind::Call);
    assert_eq!(fetch_calls.len(), 1, "`client:fetch()` is a callsite");
    assert!(
        fetch_calls[0].owners.is_empty(),
        "`client` is a lowercase receiver, so no owner is claimed"
    );

    let request = find(&tags, "request", OccKind::Call);
    assert_eq!(request.len(), 1, "`http.request()` is a callsite");

    let import = find(&tags, "socket.http", OccKind::Import);
    assert_eq!(import.len(), 1, "`require(\"socket.http\")` imports");
    assert_eq!(import[0].line, 2);
}

const BASH_SOURCE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

source ./lib/log.sh "$@"

CONFIG_DIR="${HOME}/.config"
RETRIES=3
PATHS[0]="$CONFIG_DIR"

log_dir() {
  local dir="$CONFIG_DIR"
  printf '%s\n' "$dir"
}

deploy() {
  local result
  log_dir
  rsync -a "${PATHS[0]}" "$TARGET"
}

deploy
"#;

#[test]
fn bash_functions_and_variables_are_tagged() {
    let tags = tag_source("bin/deploy.sh", BASH_SOURCE);

    let deploy = find(&tags, "deploy", OccKind::Definition);
    assert_eq!(deploy.len(), 1);
    assert_eq!(deploy[0].sym_kind, SymKind::Function);
    assert!(deploy[0].owners.is_empty(), "shell has no owning types");
    assert_eq!(deploy[0].context, None);

    let config_dir = find(&tags, "CONFIG_DIR", OccKind::Definition);
    assert_eq!(config_dir.len(), 1);
    assert_eq!(config_dir[0].sym_kind, SymKind::Variable);
    assert_eq!(config_dir[0].line, 6);

    let paths = find(&tags, "PATHS", OccKind::Definition);
    assert_eq!(paths.len(), 1, "`PATHS[0]=...` defines PATHS");
    assert_eq!(paths[0].sym_kind, SymKind::Variable);

    let dir = find(&tags, "dir", OccKind::Definition);
    assert_eq!(dir.len(), 1);
    assert_eq!(
        dir[0].context.as_deref(),
        Some("log_dir"),
        "the enclosing function names the context"
    );

    // References.
    let calls = find(&tags, "log_dir", OccKind::Call);
    assert_eq!(calls.len(), 1, "`log_dir` is called from deploy");
    assert_eq!(calls[0].context.as_deref(), Some("deploy"));

    let expansions = find(&tags, "CONFIG_DIR", OccKind::Field);
    assert_eq!(
        expansions.len(),
        2,
        "both expansions of CONFIG_DIR are reads"
    );
    assert!(expansions.iter().all(|tag| tag.owners.is_empty()));

    let subscript = find(&tags, "PATHS", OccKind::Field);
    assert_eq!(
        subscript.len(),
        1,
        "the subscripted expansion of PATHS is a read"
    );

    let import = find(&tags, "./lib/log.sh", OccKind::Import);
    assert_eq!(
        import.len(),
        1,
        "only the sourced path is an import, not the arguments after it"
    );

    let result = find(&tags, "result", OccKind::Definition);
    assert_eq!(result.len(), 1, "`local result` declares a name");
    assert_eq!(result[0].sym_kind, SymKind::Variable);
    assert_eq!(result[0].context.as_deref(), Some("deploy"));

    assert!(
        find(&tags, "printf", OccKind::Call).is_empty()
            && find(&tags, "set", OccKind::Call).is_empty()
            && find(&tags, "source", OccKind::Call).is_empty(),
        "builtins are not symbols anyone searches for"
    );
}
