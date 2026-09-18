; Definitions
;
; Nix has no declarations: every named thing is a `binding` (`foo = ...`) in an
; attrset, a `rec` attrset or a `let`. The last attr of the path is the name
; worth searching, so `services.nginx.enable = ...` is indexed as `enable`.
; Function formals (`{ lib, pkgs, ... }:`) and lambda parameters are locals and
; stay untagged.

(binding attrpath: (attrpath attr: (identifier) @name .)) @definition.variable

; Calls

(apply_expression
  function: (variable_expression name: (identifier) @name)) @reference.call

(apply_expression
  function: (select_expression
    expression: (_) @qualifier
    attrpath: (attrpath attr: (identifier) @name .))) @reference.call

; Imports
;
; `inherit (pkgs) lib stdenv;` pulls names in from another scope, one tag per
; inherited attr. `import ./foo.nix` and `callPackage ./foo.nix {}` name a
; file, which is the searchable thing about them.

(inherit_from
  expression: (_) @qualifier
  attrs: (inherited_attrs attr: (identifier) @name)) @reference.import

(inherit attrs: (inherited_attrs attr: (identifier) @name)) @reference.import

(apply_expression
  function: (variable_expression name: (identifier) @_fn)
  argument: [(path_expression) (spath_expression)] @name
  (#any-of? @_fn "import" "callPackage")) @reference.import

(apply_expression
  function: (select_expression
    attrpath: (attrpath attr: (identifier) @_fn .))
  argument: [(path_expression) (spath_expression)] @name
  (#any-of? @_fn "import" "callPackage")) @reference.import

; Attribute access
;
; `cfg.enable`, `pkgs.hello`: only the final attr is a name; the object is the
; qualifier, which resolves to an owner only when it is capitalised.

(select_expression
  expression: (_) @qualifier
  attrpath: (attrpath attr: (identifier) @name .)) @reference.field
