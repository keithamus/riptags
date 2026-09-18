; A Zig container is an anonymous value (`const Server = struct { ... }`), so
; the binding that names it opens the scope that owns its members: see
; `ZIG_SCOPES` in src/lang.rs. The patterns below only have to name each
; member; the owner follows from the enclosing binding.

; Definitions

(function_declaration name: (identifier) @name) @definition.function

; A container bound to a name is that name's type: `const Point = struct { ... }`.
(variable_declaration (identifier) @name
  [
    (struct_declaration)
    (union_declaration)
    (enum_declaration)
    (opaque_declaration)
  ]) @definition.class

(variable_declaration (identifier) @name (error_set_declaration)) @definition.type

; Struct fields, union variants and enum tags.

(container_field name: (identifier) @name) @definition.field

; Members of an error set: `const Error = error{ OutOfRange };`.

(error_set_declaration (identifier) @name) @definition.constant

; Container-level constants and variables. A local has the same node kind, but
; hangs off a `block`, so anchoring the parent keeps function bodies out. The
; bound name is the first named child after the keyword - without the anchor
; `const Alias = Packed;` would also tag `Packed` as a definition. The text
; guard drops the bindings already tagged as a class or an error set above: two
; `@definition.*` matches on one token would otherwise race in `dedupe`, and the
; loser is decided by which pattern completes first.

(source_file
  (variable_declaration "const" . (identifier) @name) @definition.constant @_decl
  (#not-match? @_decl "= *(extern |packed )?(struct|union|enum|opaque|error)\\s*[({]"))
(struct_declaration
  (variable_declaration "const" . (identifier) @name) @definition.constant @_decl
  (#not-match? @_decl "= *(extern |packed )?(struct|union|enum|opaque|error)\\s*[({]"))
(union_declaration
  (variable_declaration "const" . (identifier) @name) @definition.constant @_decl
  (#not-match? @_decl "= *(extern |packed )?(struct|union|enum|opaque|error)\\s*[({]"))
(enum_declaration
  (variable_declaration "const" . (identifier) @name) @definition.constant @_decl
  (#not-match? @_decl "= *(extern |packed )?(struct|union|enum|opaque|error)\\s*[({]"))
(opaque_declaration
  (variable_declaration "const" . (identifier) @name) @definition.constant @_decl
  (#not-match? @_decl "= *(extern |packed )?(struct|union|enum|opaque|error)\\s*[({]"))

[
  (source_file (variable_declaration "var" . (identifier) @name) @definition.variable)
  (struct_declaration (variable_declaration "var" . (identifier) @name) @definition.variable)
  (union_declaration (variable_declaration "var" . (identifier) @name) @definition.variable)
  (enum_declaration (variable_declaration "var" . (identifier) @name) @definition.variable)
  (opaque_declaration (variable_declaration "var" . (identifier) @name) @definition.variable)
]

; Calls

(call_expression function: (identifier) @name) @reference.call

(call_expression
  function: (field_expression
    object: (_) @qualifier
    member: (identifier) @name)) @reference.call

; Imports: `@import("std")`. Only module-style arguments are symbols; relative
; paths (`@import("./util.zig")`) belong to a path search.

(builtin_function
  (builtin_identifier) @_builtin
  (arguments (string (string_content) @name))
  (#eq? @_builtin "@import")
  (#match? @name "^[A-Za-z_][A-Za-z0-9_]*$")) @reference.import

; Member access. The objectless form also covers initializer designators
; (`Server{ .port = 1 }`) and enum literals (`.fast`).

(field_expression member: (identifier) @name) @reference.field

(field_expression
  object: (_) @qualifier
  member: (identifier) @name) @reference.field

; `error.Bad` names a member of an error set.

(error_type (identifier) @name) @reference.field

; Type mentions. The wrapper patterns are position-independent, so they cover
; `*Server`, `[]Server`, `[4]Server`, `?Server` and `Error!Server` wherever they
; appear; the `type:` patterns cover a bare `Server`.

(parameter type: (identifier) @name) @reference.type
(container_field type: (identifier) @name) @reference.type
(function_declaration type: (identifier) @name) @reference.type
(variable_declaration type: (identifier) @name) @reference.type

(pointer_type (identifier) @name) @reference.type
(slice_type (identifier) @name) @reference.type
(array_type (identifier) @name) @reference.type
(nullable_type (identifier) @name) @reference.type
(error_union_type error: (identifier) @name) @reference.type
(error_union_type ok: (identifier) @name) @reference.type

; `Point{ .x = 1 }` mentions the type it instantiates.
(struct_initializer (identifier) @name) @reference.type

; A TitleCase binding of a bare identifier is a type alias: `const Foo = Bar;`.
(variable_declaration (identifier) (identifier) @name
  (#match? @name "^[A-Z]")) @reference.type

; `test decltest.parse { ... }` names the declaration under test; `test "..."`
; has no symbol name.
(test_declaration (identifier) @name) @reference.path
