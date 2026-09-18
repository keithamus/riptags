; Definitions

(class_declaration name: (identifier) @name) @definition.class
(struct_declaration name: (identifier) @name) @definition.class
(record_declaration name: (identifier) @name) @definition.class
(enum_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(delegate_declaration name: (identifier) @name) @definition.type

(namespace_declaration name: (identifier) @name) @definition.module
(namespace_declaration
  name: (qualified_name
    qualifier: (_) @qualifier
    name: (identifier) @name)) @definition.module
(file_scoped_namespace_declaration name: (identifier) @name) @definition.module
(file_scoped_namespace_declaration
  name: (qualified_name
    qualifier: (_) @qualifier
    name: (identifier) @name)) @definition.module

(method_declaration name: (identifier) @name body: (_)) @definition.method
(method_declaration name: (identifier) @name !body) @declaration.method
(constructor_declaration name: (identifier) @name body: (_)) @definition.method
(constructor_declaration name: (identifier) @name !body) @declaration.method
(local_function_statement name: (identifier) @name) @definition.function

; A property is a definition wherever it carries an implementation: only an
; interface member or an `abstract` member merely declares one.
((class_declaration
  body: (declaration_list
    (property_declaration name: (identifier) @name) @definition.field))
  (#not-match? @definition.field "\\babstract\\b"))
((struct_declaration
  body: (declaration_list
    (property_declaration name: (identifier) @name) @definition.field))
  (#not-match? @definition.field "\\babstract\\b"))
((record_declaration
  body: (declaration_list
    (property_declaration name: (identifier) @name) @definition.field))
  (#not-match? @definition.field "\\babstract\\b"))
(interface_declaration
  body: (declaration_list
    (property_declaration name: (identifier) @name) @declaration.field))
((property_declaration name: (identifier) @name) @declaration.field
  (#match? @declaration.field "\\babstract\\b"))

; `const int Max = 8;` is a constant; every other field is a field.
((field_declaration
  (variable_declaration (variable_declarator name: (identifier) @name))) @definition.constant
  (#match? @definition.constant "\\bconst\\b"))
((field_declaration
  (variable_declaration (variable_declarator name: (identifier) @name))) @definition.field
  (#not-match? @definition.field "\\bconst\\b"))
(event_field_declaration
  (variable_declaration (variable_declarator name: (identifier) @name))) @definition.field
(event_declaration name: (identifier) @name accessors: (_)) @definition.field
(event_declaration name: (identifier) @name !accessors) @declaration.field
(enum_member_declaration name: (identifier) @name) @definition.constant

; An explicit interface implementation (`void IFoo.Bar() {}`) names the
; interface it satisfies; the definition itself keeps its enclosing class.
(explicit_interface_specifier (identifier) @name) @reference.impl

; Calls

(invocation_expression function: (identifier) @name) @reference.call
(invocation_expression function: (generic_name (identifier) @name)) @reference.call

(invocation_expression
  function: (member_access_expression
    expression: (_) @qualifier
    name: (identifier) @name)) @reference.call
; `this` is an anonymous token, so `(_)` never matches it.
(invocation_expression
  function: (member_access_expression
    expression: "this" @qualifier
    name: (identifier) @name)) @reference.call
(invocation_expression
  function: (member_access_expression
    expression: (_) @qualifier
    name: (generic_name (identifier) @name))) @reference.call

(invocation_expression
  function: (qualified_name
    qualifier: (_) @qualifier
    name: (identifier) @name)) @reference.call

(invocation_expression
  function: (conditional_access_expression
    condition: (_) @qualifier
    (member_binding_expression name: (identifier) @name))) @reference.call

(object_creation_expression type: (identifier) @name) @reference.call
(object_creation_expression type: (generic_name (identifier) @name)) @reference.call
(object_creation_expression
  type: (qualified_name
    qualifier: (_) @qualifier
    name: (identifier) @name)) @reference.call

; Implementations

(base_list (identifier) @name) @reference.impl
(base_list (generic_name (identifier) @name)) @reference.impl
(base_list
  (qualified_name
    qualifier: (_) @qualifier
    name: (identifier) @name)) @reference.impl
(base_list (primary_constructor_base_type type: (identifier) @name)) @reference.impl

; Imports

(using_directive (identifier) @name) @reference.import
(using_directive
  (qualified_name
    qualifier: (_) @qualifier
    name: (identifier) @name)) @reference.import

; Other mentions

(member_access_expression
  expression: (_) @qualifier
  name: (identifier) @name) @reference.field
(member_access_expression
  expression: "this" @qualifier
  name: (identifier) @name) @reference.field
(conditional_access_expression
  condition: (_) @qualifier
  (member_binding_expression name: (identifier) @name)) @reference.field

(variable_declaration type: (identifier) @name) @reference.type
(variable_declaration type: (generic_name (identifier) @name)) @reference.type
(parameter type: (identifier) @name) @reference.type
(parameter type: (generic_name (identifier) @name)) @reference.type
(method_declaration returns: (identifier) @name) @reference.type
(method_declaration returns: (generic_name (identifier) @name)) @reference.type
(property_declaration type: (identifier) @name) @reference.type
(property_declaration type: (generic_name (identifier) @name)) @reference.type
(catch_declaration type: (identifier) @name) @reference.type
(type_argument_list (identifier) @name) @reference.type
(type_argument_list (generic_name (identifier) @name)) @reference.type
(array_type type: (identifier) @name) @reference.type
(nullable_type type: (identifier) @name) @reference.type
(attribute name: (identifier) @name) @reference.type

(qualified_name
  qualifier: (_) @qualifier
  name: (identifier) @name) @reference.path
