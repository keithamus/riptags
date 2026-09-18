; Definitions

(class_declaration
  declaration_kind: ["class" "struct" "enum" "actor"]
  name: (type_identifier) @name) @definition.class

(protocol_declaration
  name: (type_identifier) @name) @definition.interface

(typealias_declaration
  name: (type_identifier) @name) @definition.type

(associatedtype_declaration
  name: (type_identifier) @name) @declaration.type

(macro_declaration (simple_identifier) @name) @definition.macro

(source_file
  (function_declaration
    name: (simple_identifier) @name)) @definition.function

(class_body
  (function_declaration
    name: (simple_identifier) @name)) @definition.method

(enum_class_body
  (function_declaration
    name: (simple_identifier) @name)) @definition.method

(init_declaration "init" @name body: (_)) @definition.method
(init_declaration "init" @name !body) @declaration.method
(deinit_declaration "deinit" @name) @definition.method

(protocol_function_declaration
  name: (simple_identifier) @name) @declaration.method

(class_body
  (property_declaration
    name: (pattern bound_identifier: (simple_identifier) @name))) @definition.field

(enum_class_body
  (property_declaration
    name: (pattern bound_identifier: (simple_identifier) @name))) @definition.field

(protocol_property_declaration
  name: (pattern bound_identifier: (simple_identifier) @name)) @declaration.field

(source_file
  (property_declaration
    (value_binding_pattern mutability: "let")
    name: (pattern bound_identifier: (simple_identifier) @name))) @definition.constant

(source_file
  (property_declaration
    (value_binding_pattern mutability: "var")
    name: (pattern bound_identifier: (simple_identifier) @name))) @definition.variable

(enum_entry
  name: (simple_identifier) @name) @definition.field

; Calls

(call_expression
  . (simple_identifier) @name) @reference.call

(call_expression
  . (navigation_expression
      target: (_) @qualifier
      suffix: (navigation_suffix suffix: (simple_identifier) @name))) @reference.call

(constructor_expression
  constructed_type: (user_type (type_identifier) @name)) @reference.call

(macro_invocation (simple_identifier) @name) @reference.call

; Implementations

(inheritance_specifier
  inherits_from: (user_type (type_identifier) @name)) @reference.impl

(class_declaration
  declaration_kind: "extension"
  name: (type_identifier) @name) @reference.impl

(class_declaration
  declaration_kind: "extension"
  name: (user_type (type_identifier) @name)) @reference.impl

; Imports

(import_declaration (identifier . (simple_identifier) @name)) @reference.import

; Other mentions

(navigation_expression
  target: (_) @qualifier
  suffix: (navigation_suffix suffix: (simple_identifier) @name)) @reference.field

(type_identifier) @name @reference.type
