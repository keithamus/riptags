; Definitions
;
; Note on shape: a field-labelled child step (`type: (type_identifier)`) binds
; only the *first* child under that field, so repeated fields - the `type:` of
; an `extends_clause`, the `path:` of an `import_declaration` - are matched
; without the label. Anchors (`.`) likewise only take effect on unlabelled
; steps, which is how the last segment of an import path is picked out.

(package_clause name: (package_identifier) @name) @definition.module

(class_definition name: (_) @name) @definition.class
(object_definition name: (_) @name) @definition.class
(enum_definition name: (_) @name) @definition.class
(trait_definition name: (_) @name) @definition.interface

(full_enum_case name: (_) @name) @definition.class
(simple_enum_case name: (_) @name) @definition.constant

(type_definition name: (type_identifier) @name) @definition.type

; `def` is a method when it sits in a template (class, object, trait, enum or
; given body), a plain function at the top level of a file. A `def` nested in
; an expression block is a local and stays untagged.

; A `given ... with { }` body is a `with_template_body`, not a `template_body`.

[
  (template_body (function_definition name: (_) @name) @definition.method)
  (with_template_body (function_definition name: (_) @name) @definition.method)
]

[
  (template_body (function_declaration name: (_) @name) @declaration.method)
  (with_template_body (function_declaration name: (_) @name) @declaration.method)
]

(compilation_unit
  (function_definition name: (_) @name) @definition.function)

; An extension method is owned by the type it extends, which the definition
; itself names - `extension (s: Shape) def twice` gives `Shape#twice`. A
; qualified or generic receiver answers as its base type.

(extension_definition
  (function_definition name: (_) @name) @definition.method)

(extension_definition
  parameters: (parameters
    (parameter
      type: [
        (type_identifier) @qualifier
        (stable_type_identifier (type_identifier) @qualifier)
        (generic_type type: (type_identifier) @qualifier)
      ]))
  (function_definition name: (_) @name) @definition.method)

; `val`/`var`/`given` members of a template. Locals inside a body stay untagged.

[
  (template_body (val_definition pattern: (identifier) @name) @definition.field)
  (with_template_body (val_definition pattern: (identifier) @name) @definition.field)
]

[
  (template_body (var_definition pattern: (identifier) @name) @definition.field)
  (with_template_body (var_definition pattern: (identifier) @name) @definition.field)
]

(template_body
  (val_declaration (identifier) @name) @declaration.field)

(template_body
  (var_declaration (identifier) @name) @declaration.field)

(template_body
  (given_definition name: (identifier) @name) @definition.field)

(compilation_unit
  (given_definition name: (identifier) @name) @definition.constant)

; A `val`/`var` class parameter is a public member; a bare parameter is not.

(class_parameter
  "val"
  name: (_) @name) @definition.field

(class_parameter
  "var"
  name: (_) @name) @definition.field

; Every parameter of a `case class` is a public member.

(class_definition
  "case"
  class_parameters: (class_parameters
    (class_parameter name: (_) @name) @definition.field))

; Calls

(call_expression
  function: (identifier) @name) @reference.call

(call_expression
  function: (field_expression
    value: (_) @qualifier
    field: (identifier) @name)) @reference.call

(call_expression
  function: (generic_function
    function: (identifier) @name)) @reference.call

(call_expression
  function: (generic_function
    function: (field_expression
      value: (_) @qualifier
      field: (identifier) @name))) @reference.call

; Instantiation: `new Foo`, `new Foo[T]`, `new pkg.Foo`

(instance_expression
  (type_identifier) @name) @reference.call

(instance_expression
  (generic_type
    type: (type_identifier) @name)) @reference.call

(instance_expression
  (stable_type_identifier
    (_) @qualifier
    (type_identifier) @name)) @reference.call

; Inheritance: `extends A with B`, `derives C`

(extends_clause
  (type_identifier) @name) @reference.impl

(extends_clause
  (generic_type
    type: (type_identifier) @name)) @reference.impl

(extends_clause
  (stable_type_identifier
    (_) @qualifier
    (type_identifier) @name)) @reference.impl

(derives_clause
  (type_identifier) @name) @reference.impl

; Imports: the last path segment, plus every selector of `import a.b.{C, D => E}`

(import_declaration
  (identifier) @name .) @reference.import

(import_declaration
  (identifier) @qualifier .
  (identifier) @name .) @reference.import

(import_declaration
  (identifier) @name .
  (namespace_wildcard)) @reference.import

(import_declaration
  (namespace_selectors (identifier) @name)) @reference.import

(import_declaration
  (identifier) @qualifier .
  (namespace_selectors (identifier) @name)) @reference.import

(import_declaration
  (namespace_selectors
    (arrow_renamed_identifier name: (identifier) @name))) @reference.import

(import_declaration
  (namespace_selectors
    (as_renamed_identifier name: (identifier) @name))) @reference.import

; Member selection and type mentions

(field_expression
  value: (_) @qualifier
  field: (identifier) @name) @reference.field

(stable_type_identifier
  (_) @qualifier
  (type_identifier) @name) @reference.type

(type_identifier) @name @reference.type
