; Definitions

(function_declaration name: (identifier) @name) @definition.function
(generator_function_declaration name: (identifier) @name) @definition.function
(method_definition name: (property_identifier) @name) @definition.method
(class_declaration name: (identifier) @name) @definition.class
(field_definition property: (property_identifier) @name) @definition.field

(variable_declarator
  name: (identifier) @name
  value: [(arrow_function) (function_expression)]) @definition.function

(variable_declarator name: (identifier) @name) @definition.variable

(pair
  key: (property_identifier) @name
  value: [(function_expression) (arrow_function)]) @definition.method

; Calls

(call_expression function: (identifier) @name) @reference.call

(call_expression
  function: (member_expression
    object: (_) @qualifier
    property: (property_identifier) @name)) @reference.call

(new_expression constructor: (identifier) @name) @reference.call

; Imports

(import_specifier name: (identifier) @name) @reference.import

; Other mentions

(member_expression
  object: (_) @qualifier
  property: (property_identifier) @name) @reference.field
