; Modules
;---------

(module_definition (module_binding (module_name) @name)) @definition.module
(module_type_definition (module_type_name) @name) @definition.interface

; A module, class or type is a scope (see `OCAML_SCOPES` in src/lang.rs), so
; its members take their owner from it: OCaml spells them `Cache.make`, and
; that is how they are searchable.

; Types
;-------

(type_definition (type_binding name: (type_constructor) @name)) @definition.type

(field_declaration (field_name) @name) @definition.field
(constructor_declaration (constructor_name) @name) @definition.field
(tag_specification (tag) @name) @definition.field

; Classes
;---------

(class_definition (class_binding (class_name) @name)) @definition.class
(class_type_definition (class_type_binding (class_type_name) @name)) @definition.interface
(method_definition (method_name) @name) @definition.method
(instance_variable_definition (instance_variable_name) @name) @definition.field

; Values
;--------

; A binding is a function when it takes parameters or its body is a `fun` or
; `function` expression, and a constant otherwise. The three groups are
; mutually exclusive: the constant group anchors the name directly against the
; body (so no parameter can sit between them) and rejects function bodies by
; text. Bindings under a `let ... in` are locals - indistinguishable from a
; variable read - so only structure-level bindings are tagged.

[
  (compilation_unit
    (value_definition (let_binding pattern: (value_name) @name (parameter))))
  (structure
    (value_definition (let_binding pattern: (value_name) @name (parameter))))
] @definition.function

[
  (compilation_unit
    (value_definition
      (let_binding
        pattern: (value_name) @name
        body: [(fun_expression) (function_expression)])))
  (structure
    (value_definition
      (let_binding
        pattern: (value_name) @name
        body: [(fun_expression) (function_expression)])))
] @definition.function

(
  [
    (compilation_unit
      (value_definition (let_binding pattern: (value_name) @name . body: (_) @body)))
    (structure
      (value_definition (let_binding pattern: (value_name) @name . body: (_) @body)))
    (compilation_unit
      (value_definition
        (let_binding pattern: (value_name) @name . type: (_) . body: (_) @body)))
    (structure
      (value_definition
        (let_binding pattern: (value_name) @name . type: (_) . body: (_) @body)))
  ] @definition.constant
  (#not-match? @body "^(fun|function)\\b")
)

[
  (compilation_unit
    (value_definition (let_binding pattern: (parenthesized_operator (_) @name))))
  (structure
    (value_definition (let_binding pattern: (parenthesized_operator (_) @name))))
] @definition.function

; Declarations
;--------------

; `val f : int -> int` and `external now : unit -> float` declare functions,
; every other signature entry declares a value. The two predicates are
; complements, so exactly one of them matches.

(
  [
    (value_specification (value_name) @name type: (_) @ty)
    (external (value_name) @name type: (_) @ty)
  ] @declaration.function
  (#match? @ty "->")
)

(
  [
    (value_specification (value_name) @name type: (_) @ty)
    (external (value_name) @name type: (_) @ty)
  ] @declaration.constant
  (#not-match? @ty "->")
)

; Calls
;-------

(application_expression function: (value_path . (value_name) @name)) @reference.call
(application_expression
  function: (value_path
    (module_path) @qualifier
    (value_name) @name)) @reference.call
(method_invocation object: (_) @qualifier method: (method_name) @name) @reference.call

; Imports
;---------

(open_module module: (module_path (module_name) @name)) @reference.import
(open_module
  module: (module_path (module_path) @qualifier (module_name) @name)) @reference.import
(include_module module: (module_path (module_name) @name)) @reference.import
(include_module
  module: (module_path (module_path) @qualifier (module_name) @name)) @reference.import
(open_module_signature
  module: (extended_module_path (module_name) @name)) @reference.import

; Other mentions
;----------------

; A qualified value (`Cache.limit`) names something in another module; a bare
; value path is a local read and stays untagged.
(value_path (module_path) @qualifier (value_name) @name) @reference.path

(module_path (module_name) @name) @reference.path
(module_path (module_path) @qualifier (module_name) @name) @reference.path
(extended_module_path (module_name) @name) @reference.path
(extended_module_path
  (extended_module_path) @qualifier
  (module_name) @name) @reference.path

(type_constructor_path (type_constructor) @name) @reference.type
(type_constructor_path
  (extended_module_path) @qualifier
  (type_constructor) @name) @reference.type

(class_path (class_name) @name) @reference.type
(class_path (module_path) @qualifier (class_name) @name) @reference.type
(class_type_path (class_type_name) @name) @reference.type
(module_type_path (module_type_name) @name) @reference.type
(instance_variable_expression (instance_variable_name) @name) @reference.field

(constructor_path (constructor_name) @name) @reference.path
(constructor_path
  (module_path) @qualifier
  (constructor_name) @name) @reference.path

(field_path (field_name) @name) @reference.field
(field_path (module_path) @qualifier (field_name) @name) @reference.field
