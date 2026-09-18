; Definitions

(function_declaration name: (identifier) @name) @definition.function
(method_declaration name: (field_identifier) @name) @definition.method
(type_spec name: (type_identifier) @name type: (struct_type)) @definition.class
(type_spec name: (type_identifier) @name type: (interface_type)) @definition.interface
(type_spec name: (type_identifier) @name) @definition.type
(type_alias name: (type_identifier) @name) @definition.type
(const_spec name: (identifier) @name) @definition.constant
(var_spec name: (identifier) @name) @definition.variable
(method_elem name: (field_identifier) @name) @declaration.method
(field_declaration name: (field_identifier) @name) @definition.field

; Calls

(call_expression function: (identifier) @name) @reference.call

(call_expression
  function: (selector_expression
    operand: (_) @qualifier
    field: (field_identifier) @name)) @reference.call

; Other mentions

(selector_expression
  operand: (_) @qualifier
  field: (field_identifier) @name) @reference.field

(type_identifier) @name @reference.type
