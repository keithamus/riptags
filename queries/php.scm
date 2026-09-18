; Definitions

(class_declaration name: (name) @name) @definition.class
(enum_declaration name: (name) @name) @definition.class
(interface_declaration name: (name) @name) @definition.interface
(trait_declaration name: (name) @name) @definition.interface
(namespace_definition name: (namespace_name (name) @name .)) @definition.module

(function_definition name: (name) @name) @definition.function

(method_declaration name: (name) @name body: (compound_statement)) @definition.method
(method_declaration name: (name) @name !body) @declaration.method

(property_declaration
  (property_element name: (variable_name (name) @name))) @definition.field
(const_declaration (const_element (name) @name)) @definition.constant
(property_promotion_parameter
  name: (variable_name (name) @name)) @definition.field

; `$normalize = function () {}` and `$double = fn($x) => $x;` bind a callable.
(assignment_expression
  left: (variable_name (name) @name)
  right: [
    (anonymous_function)
    (arrow_function)
  ]) @definition.function
(enum_case name: (name) @name) @definition.constant

; Calls

(function_call_expression function: (name) @name) @reference.call
(function_call_expression
  function: (qualified_name
    prefix: (_) @qualifier
    (name) @name)) @reference.call

; `\count($items)` calls the root namespace: the prefix is an anonymous token,
; so there is no qualifier to bind.
(function_call_expression
  function: (qualified_name (name) @name)) @reference.call

(member_call_expression
  object: (variable_name (name) @qualifier)
  name: (name) @name) @reference.call
(member_call_expression
  object: (_) @qualifier
  name: (name) @name) @reference.call
(nullsafe_member_call_expression
  object: (variable_name (name) @qualifier)
  name: (name) @name) @reference.call
(nullsafe_member_call_expression
  object: (_) @qualifier
  name: (name) @name) @reference.call

(scoped_call_expression
  scope: (name) @qualifier
  name: (name) @name) @reference.call
(scoped_call_expression
  scope: (qualified_name (name) @qualifier)
  name: (name) @name) @reference.call

; `self::`, `static::` and `parent::` are one node, not a name.
(scoped_call_expression
  scope: (relative_scope) @qualifier
  name: (name) @name) @reference.call

(object_creation_expression (name) @name) @reference.call
(object_creation_expression
  (qualified_name
    prefix: (_) @qualifier
    (name) @name)) @reference.call

; Implementations

(base_clause [(name) @name (qualified_name (name) @name)]) @reference.impl
(class_interface_clause [(name) @name (qualified_name (name) @name)]) @reference.impl
(use_declaration [(name) @name (qualified_name (name) @name)]) @reference.impl

; Imports

(namespace_use_clause (name) @name) @reference.import
(namespace_use_clause
  (qualified_name
    prefix: (_) @qualifier
    (name) @name)) @reference.import

; `require 'src/Order.php'` names the file it pulls in.
(require_expression (string (string_content) @name)) @reference.import
(require_once_expression (string (string_content) @name)) @reference.import
(include_expression (string (string_content) @name)) @reference.import
(include_once_expression (string (string_content) @name)) @reference.import

; Other mentions

(member_access_expression
  object: (variable_name (name) @qualifier)
  name: (name) @name) @reference.field
(member_access_expression
  object: (_) @qualifier
  name: (name) @name) @reference.field
(nullsafe_member_access_expression
  object: (variable_name (name) @qualifier)
  name: (name) @name) @reference.field
(nullsafe_member_access_expression
  object: (_) @qualifier
  name: (name) @name) @reference.field

(scoped_property_access_expression
  scope: (name) @qualifier
  name: (variable_name (name) @name)) @reference.field
(class_constant_access_expression (name) @qualifier (name) @name) @reference.field

(named_type [(name) @name (qualified_name (name) @name)]) @reference.type
