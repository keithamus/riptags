; Definitions

(function_item name: (identifier) @name) @definition.function
(function_signature_item name: (identifier) @name) @declaration.function
(struct_item name: (type_identifier) @name) @definition.class
(enum_item name: (type_identifier) @name) @definition.class
(union_item name: (type_identifier) @name) @definition.class
(type_item name: (type_identifier) @name) @definition.type
(trait_item name: (type_identifier) @name) @definition.interface
(mod_item name: (identifier) @name) @definition.module
(macro_definition name: (identifier) @name) @definition.macro
(const_item name: (identifier) @name value: (_)) @definition.constant
(const_item name: (identifier) @name !value) @declaration.constant
(static_item name: (identifier) @name value: (_)) @definition.constant
(static_item name: (identifier) @name !value) @declaration.constant
(associated_type name: (type_identifier) @name) @declaration.type
(field_declaration name: (field_identifier) @name) @definition.field
(enum_variant name: (identifier) @name) @definition.field

; Calls

(call_expression function: (identifier) @name) @reference.call

(call_expression
  function: (field_expression
    value: (_) @qualifier
    field: (field_identifier) @name)) @reference.call

(call_expression
  function: (scoped_identifier
    path: (_) @qualifier
    name: (identifier) @name)) @reference.call

(call_expression
  function: (generic_function
    function: (identifier) @name)) @reference.call

(call_expression
  function: (generic_function
    function: (scoped_identifier
      path: (_) @qualifier
      name: (identifier) @name))) @reference.call

(macro_invocation macro: (identifier) @name) @reference.call

(macro_invocation
  macro: (scoped_identifier
    path: (_) @qualifier
    name: (identifier) @name)) @reference.call

; Implementations

(impl_item trait: (type_identifier) @name) @reference.impl
(impl_item type: (type_identifier) @name) @reference.impl

; Imports

(use_declaration
  argument: (scoped_identifier
    path: (_) @qualifier
    name: (identifier) @name)) @reference.import

; `use foo::{Bar, Baz as Q};` imports every member of the group.

(use_list (identifier) @name) @reference.import
(use_list (scoped_identifier name: (identifier) @name)) @reference.import
(use_as_clause alias: (identifier) @name) @reference.import
(use_as_clause path: (identifier) @name) @reference.import
(use_as_clause
  path: (scoped_identifier name: (identifier) @name)) @reference.import

; Other mentions

(scoped_identifier
  path: (_) @qualifier
  name: (identifier) @name) @reference.path

(scoped_type_identifier
  path: (_) @qualifier
  name: (type_identifier) @name) @reference.type

(type_identifier) @name @reference.type

(field_expression
  value: (_) @qualifier
  field: (field_identifier) @name) @reference.field
