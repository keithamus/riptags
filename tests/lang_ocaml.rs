#![cfg(feature = "ocaml")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, one, owners};

const SRC: &str = r#"
open Printf
open Core.Std

module Cache = struct
  type entry = { key : string; size : int }

  type status = Hit | Miss

  let make key = { key; size = 0 }

  let limit = 128

  class counter = object
    method bump n = n + 1
  end
end

module type S = sig
  val lookup : string -> int
  val version : string
end

include Cache

external now : unit -> float = "caml_sys_time"

let boot () =
  let c = Cache.make "a" in
  printf "%d" c.size;
  ignore (Cache.limit);
  c.key
"#;

#[test]
fn ocaml_definitions_owners_and_references() {
    let tags = tag_source("cache.ml", SRC);

    // A module is a definition of kind module, and its structure items are
    // owned by it: OCaml spells them `Cache.make`, so that is how they are
    // searchable.
    let cache = one(&tags, "Cache", OccKind::Definition);
    assert_eq!(cache.sym_kind, SymKind::Module);
    assert_eq!(cache.line, 5);
    assert_eq!(cache.col, 7);
    assert_eq!(cache.len, 5);

    // `let` with parameters is a function, a plain binding is a constant.
    // The enclosing module owns both and names their context.
    let make = one(&tags, "make", OccKind::Definition);
    assert_eq!(make.sym_kind, SymKind::Function);
    assert_eq!(owners(make), ["Cache"]);
    assert_eq!(make.context.as_deref(), Some("Cache"));
    assert_eq!((make.line, make.col), (10, 6));

    let limit = one(&tags, "limit", OccKind::Definition);
    assert_eq!(limit.sym_kind, SymKind::Constant);
    assert_eq!(owners(limit), ["Cache"]);

    // A top-level binding has no enclosing scope at all.
    let boot = one(&tags, "boot", OccKind::Definition);
    assert_eq!(boot.sym_kind, SymKind::Function);
    assert_eq!(owners(boot), [] as [&str; 0]);
    assert_eq!(boot.context, None);

    // A `type` definition, its record fields and its variant constructors.
    let entry = one(&tags, "entry", OccKind::Definition);
    assert_eq!(entry.sym_kind, SymKind::Type);
    assert_eq!(owners(entry), ["Cache"]);

    let key = one(&tags, "key", OccKind::Definition);
    assert_eq!(key.sym_kind, SymKind::Field);
    assert_eq!(owners(key), ["entry"], "a record field belongs to its type");
    assert_eq!((key.line, key.col), (6, 17));

    let hit = one(&tags, "Hit", OccKind::Definition);
    assert_eq!(hit.sym_kind, SymKind::Field);
    assert_eq!(owners(hit), ["status"]);

    // Classes and methods.
    let counter = one(&tags, "counter", OccKind::Definition);
    assert_eq!(counter.sym_kind, SymKind::Class);
    assert_eq!(owners(counter), ["Cache"]);

    let bump = one(&tags, "bump", OccKind::Definition);
    assert_eq!(bump.sym_kind, SymKind::Method);
    assert_eq!(owners(bump), ["counter"], "a method belongs to its class");
    assert_eq!(bump.context.as_deref(), Some("Cache::counter"));

    // Module types are interfaces; `val` and `external` are declarations, and
    // an arrow in the type makes them functions.
    let sig_s = one(&tags, "S", OccKind::Definition);
    assert_eq!(sig_s.sym_kind, SymKind::Interface);

    let lookup = one(&tags, "lookup", OccKind::Declaration);
    assert_eq!(lookup.sym_kind, SymKind::Function);

    let version = one(&tags, "version", OccKind::Declaration);
    assert_eq!(version.sym_kind, SymKind::Constant);

    let now = one(&tags, "now", OccKind::Declaration);
    assert_eq!(now.sym_kind, SymKind::Function);

    // A qualified call resolves its owner from the module path.
    let call = one(&tags, "make", OccKind::Call);
    assert_eq!(owners(call), ["Cache"]);
    assert_eq!(
        call.context.as_deref(),
        Some("boot"),
        "a `let ... in` binds a value, so the enclosing function names it"
    );
    assert_eq!((call.line, call.col), (29, 16));

    // A qualified value that is not applied is a plain path reference.
    let limit_use = one(&tags, "limit", OccKind::Path);
    assert_eq!(owners(limit_use), ["Cache"]);
    assert_eq!(limit_use.line, 31);

    // An unqualified application is a call without an owner.
    let printf = one(&tags, "printf", OccKind::Call);
    assert_eq!(owners(printf), [] as [&str; 0]);

    // `open` and `include` are imports; `open Core.Std` keeps `Core` as the
    // owner of `Std`.
    let printf_import = one(&tags, "Printf", OccKind::Import);
    assert_eq!(printf_import.line, 2);
    assert_eq!(printf_import.col, 5);

    let std = one(&tags, "Std", OccKind::Import);
    assert_eq!(owners(std), ["Core"]);

    let include = one(&tags, "Cache", OccKind::Import);
    assert_eq!(include.line, 24);

    // Record field access: both the `{ key; size = 0 }` construction and the
    // `c.size` read are field references.
    let size_uses = find(&tags, "size", OccKind::Field);
    assert_eq!(
        size_uses
            .iter()
            .map(|tag| (tag.line, tag.col))
            .collect::<Vec<_>>(),
        [(10, 24), (30, 16)]
    );

    let string = find(&tags, "string", OccKind::Type);
    assert_eq!(string.len(), 3);
    assert!(string.iter().all(|tag| tag.sym_kind == SymKind::Type));

    // A `let ... in` binding is a local: indistinguishable from a variable
    // read, so it is never tagged.
    assert!(find(&tags, "c", OccKind::Definition).is_empty());
    assert!(tags.iter().all(|tag| &*tag.name != "c"));
    assert!(tags.iter().all(|tag| &*tag.name != "n"));
}
