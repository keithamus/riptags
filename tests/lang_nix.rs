#![cfg(feature = "nix")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, named};

const NIX_SOURCE: &str = r#"
{ lib, pkgs, config, unusedFormal, ... }:
let
  cfg = pkgs.callPackage ./pkg.nix { };
  helpers = import ./helpers.nix;
  mkUnit = name: { description = name; };
in
{
  imports = [ ./base.nix ];

  services.nginx.enable = lib.mkDefault true;
  services.nginx.virtualHosts."example.com".root = "/srv";

  inherit (pkgs) coreutils;

  unit = mkUnit "web";
  built = pkgs.stdenv.mkDerivation {
    pname = "demo";
    buildInputs = [ pkgs.hello ];
  };

  check = config.services.nginx.enable;
}
"#;

#[test]
fn nix_bindings_and_attribute_references() {
    let tags = tag_source("default.nix", NIX_SOURCE);

    // A top-level attrset binding is a definition.
    let unit = find(&tags, "unit", OccKind::Definition);
    assert_eq!(unit.len(), 1);
    assert_eq!(unit[0].sym_kind, SymKind::Variable);

    // A `let` binding is a definition too.
    let cfg = find(&tags, "cfg", OccKind::Definition);
    assert_eq!(cfg.len(), 1);
    assert_eq!(cfg[0].sym_kind, SymKind::Variable);

    // A nested attrpath is indexed by its last segment only: the leading
    // path components are attrset routing, not names worth searching.
    let enable = find(&tags, "enable", OccKind::Definition);
    assert_eq!(enable.len(), 1, "services.nginx.enable defines `enable`");
    assert_eq!(enable[0].line, 11);
    assert!(named(&tags, "services").is_empty());
    assert!(named(&tags, "nginx").is_empty());
    let root = find(&tags, "root", OccKind::Definition);
    assert_eq!(
        root.len(),
        1,
        "a quoted attr in the middle does not block it"
    );

    // Attribute access: the final attr is the name, and a lowercase receiver
    // claims no owner (`pkgs` is a variable, not a type).
    let hello = find(&tags, "hello", OccKind::Field);
    assert_eq!(hello.len(), 1);
    assert!(
        hello[0].owners.is_empty(),
        "lowercase receiver owns nothing"
    );

    // The same name can be a definition here and a use over there.
    let enable_use = find(&tags, "enable", OccKind::Field);
    assert_eq!(enable_use.len(), 1);
    assert_eq!(enable_use[0].line, 22);

    // Application callees: bare and selected.
    assert_eq!(find(&tags, "mkUnit", OccKind::Call).len(), 1);
    let mk_default = find(&tags, "mkDefault", OccKind::Call);
    assert_eq!(mk_default.len(), 1);
    assert_eq!(
        mk_default[0].context.as_deref(),
        Some("enable"),
        "a path-shaped binding names its last segment, not its first"
    );
    assert_eq!(find(&tags, "mkDerivation", OccKind::Call).len(), 1);

    // Loader arguments name a file. Both callee shapes are covered: the
    // selected `pkgs.callPackage` and the bare `import`.
    let pkg = find(&tags, "./pkg.nix", OccKind::Import);
    assert_eq!(pkg.len(), 1, "callPackage argument");
    let helpers = find(&tags, "./helpers.nix", OccKind::Import);
    assert_eq!(helpers.len(), 1, "import argument");

    // `inherit (pkgs) coreutils;` pulls a name in from another scope.
    let inherited = find(&tags, "coreutils", OccKind::Import);
    assert_eq!(inherited.len(), 1);

    // Deliberately untagged: function formals are locals, a path in a plain
    // list is data rather than an import, and string attrs are not names.
    assert!(
        named(&tags, "unusedFormal").is_empty(),
        "formals are locals, not symbols"
    );
    assert!(named(&tags, "./base.nix").is_empty());
    assert!(named(&tags, "example.com").is_empty());
    assert!(named(&tags, "\"example.com\"").is_empty());
}
