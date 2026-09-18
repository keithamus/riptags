; Definitions

(function_definition
  declarator: [
    (function_declarator declarator: (identifier) @name)
    (_ (function_declarator declarator: (identifier) @name))
    (_ (_ (function_declarator declarator: (identifier) @name)))
  ]) @definition.function

(declaration
  declarator: [
    (function_declarator declarator: (identifier) @name)
    (_ (function_declarator declarator: (identifier) @name))
    (_ (_ (function_declarator declarator: (identifier) @name)))
  ]) @declaration.function

(struct_specifier name: (type_identifier) @name body: (_)) @definition.class
(union_specifier name: (type_identifier) @name body: (_)) @definition.class
(enum_specifier name: (type_identifier) @name body: (_)) @definition.class
(type_definition declarator: (type_identifier) @name) @definition.type
(preproc_def name: (identifier) @name) @definition.macro
(preproc_function_def name: (identifier) @name) @definition.macro
(field_declaration declarator: (field_identifier) @name) @definition.field

(enum_specifier
  body: (enumerator_list
    (enumerator name: (identifier) @name))) @definition.constant

; Calls

(call_expression function: (identifier) @name) @reference.call

(call_expression
  function: (field_expression
    argument: (_) @qualifier
    field: (field_identifier) @name)) @reference.call

; Other mentions

(field_expression
  argument: (_) @qualifier
  field: (field_identifier) @name) @reference.field

(type_identifier) @name @reference.type
