; Definitions

(class_declaration name: (identifier) @name) @definition.class
(record_declaration name: (identifier) @name) @definition.class
(enum_declaration name: (identifier) @name) @definition.class
(interface_declaration name: (identifier) @name) @definition.interface
(annotation_type_declaration name: (identifier) @name) @definition.interface

(package_declaration (identifier) @name) @definition.module
(package_declaration
  (scoped_identifier
    scope: (_) @qualifier
    name: (identifier) @name)) @definition.module

(method_declaration name: (identifier) @name body: (block)) @definition.method
(method_declaration !body name: (identifier) @name) @declaration.method
(constructor_declaration name: (identifier) @name) @definition.method
(compact_constructor_declaration name: (identifier) @name) @definition.method
(annotation_type_element_declaration name: (identifier) @name) @declaration.method

; `static final` members are constants; every other field stays a field. The
; constant pattern comes first so it wins the dedupe tie on the same token.
((field_declaration
   (modifiers) @_mods
   declarator: (variable_declarator name: (identifier) @name)) @definition.constant
 (#match? @_mods "static")
 (#match? @_mods "final"))
(field_declaration
  declarator: (variable_declarator name: (identifier) @name)) @definition.field
(record_declaration
  parameters: (formal_parameters
    (formal_parameter name: (identifier) @name))) @definition.field
(constant_declaration
  declarator: (variable_declarator name: (identifier) @name)) @definition.constant
(enum_constant name: (identifier) @name) @definition.constant

; Calls

(method_invocation !object name: (identifier) @name) @reference.call

(method_invocation
  object: (_) @qualifier
  name: (identifier) @name) @reference.call

(object_creation_expression type: (type_identifier) @name) @reference.call
(object_creation_expression
  type: (generic_type (type_identifier) @name)) @reference.call
(object_creation_expression
  type: (scoped_type_identifier
    (_) @qualifier .
    (type_identifier) @name .)) @reference.call

(method_reference (_) @qualifier (identifier) @name) @reference.call

; Implementations

(superclass (type_identifier) @name) @reference.impl
(superclass (generic_type (type_identifier) @name)) @reference.impl
(superclass
  (scoped_type_identifier
    (_) @qualifier .
    (type_identifier) @name .)) @reference.impl

(type_list (type_identifier) @name) @reference.impl
(type_list (generic_type (type_identifier) @name)) @reference.impl
(type_list
  (scoped_type_identifier
    (_) @qualifier .
    (type_identifier) @name .)) @reference.impl

; Imports

; The trailing anchor keeps wildcard imports (`import java.util.*;`) out: they
; name no symbol, only a package.
(import_declaration
  (scoped_identifier
    scope: (_) @qualifier
    name: (identifier) @name) .) @reference.import

; Other mentions

(field_access
  object: (_) @qualifier
  field: (identifier) @name) @reference.field

(marker_annotation name: (identifier) @name) @reference.type
(annotation name: (identifier) @name) @reference.type
(marker_annotation
  name: (scoped_identifier
    scope: (_) @qualifier
    name: (identifier) @name)) @reference.type
(annotation
  name: (scoped_identifier
    scope: (_) @qualifier
    name: (identifier) @name)) @reference.type

; A qualified type name is a chain of `type_identifier`s, so the lowercase
; package segments are dropped: only the class-shaped tail is a type mention.
((scoped_type_identifier
   (_) @qualifier .
   (type_identifier) @name .) @reference.type
 (#match? @name "^[A-Z]"))

; Type parameters (`T`, `E`, `K`, `V`, `T1`) are locals of the signature, not
; searchable symbols, and are shaped exactly like a type mention. A single
; uppercase letter (optionally numbered) is the Java convention for them, so
; that shape is dropped - both at the declaration and at every use.
((type_identifier) @name @reference.type
 (#match? @name "^[A-Z]")
 (#not-match? @name "^[A-Z][0-9]?$"))
