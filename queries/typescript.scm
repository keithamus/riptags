; Definitions

(function_declaration name: (identifier) @name) @definition.function
(generator_function_declaration name: (identifier) @name) @definition.function
(method_definition name: (property_identifier) @name) @definition.method
(class_declaration name: (type_identifier) @name) @definition.class
(abstract_class_declaration name: (type_identifier) @name) @definition.class
(interface_declaration name: (type_identifier) @name) @definition.interface
(type_alias_declaration name: (type_identifier) @name) @definition.type
(enum_declaration name: (identifier) @name) @definition.class
(internal_module name: (identifier) @name) @definition.module
(public_field_definition name: (property_identifier) @name) @definition.field
(property_signature name: (property_identifier) @name) @declaration.field
(method_signature name: (property_identifier) @name) @declaration.function

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

(type_identifier) @name @reference.type

(member_expression
  object: (_) @qualifier
  property: (property_identifier) @name) @reference.field
