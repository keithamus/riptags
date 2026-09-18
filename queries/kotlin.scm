; Definitions
;
; `class_declaration` covers `class`, `interface`, `fun interface`, `enum
; class` and `data class`: the keyword is an anonymous token, so match it to
; tell an interface from a class. `enum class`/`data class` differ only by a
; `class_modifier`, and both are classes here.
(class_declaration "class" name: (identifier) @name) @definition.class
(class_declaration "interface" name: (identifier) @name) @definition.interface
(object_declaration name: (identifier) @name) @definition.class
(companion_object name: (identifier) @name) @definition.class
(type_alias type: (identifier) @name) @definition.type
(enum_entry (identifier) @name) @definition.constant

; A member of a concrete holder - `class`, `enum class`, `object`, `companion
; object`, anonymous `object : T {}` - defines; every member of an
; `interface`/`fun interface` only declares, as does any bodiless function
; (`abstract`, `external`, `expect`) and any `abstract` property. A member
; function is a method; a top-level or local one is a function. Locals inside a
; `block` stay untagged.

(class_declaration "class"
  (class_body
    (function_declaration name: (identifier) @name (function_body)) @definition.method))
(class_declaration
  (enum_class_body
    (function_declaration name: (identifier) @name (function_body)) @definition.method))
(object_declaration
  (class_body
    (function_declaration name: (identifier) @name (function_body)) @definition.method))
(companion_object
  (class_body
    (function_declaration name: (identifier) @name (function_body)) @definition.method))
(object_literal
  (class_body
    (function_declaration name: (identifier) @name (function_body)) @definition.method))

(class_body
  (function_declaration name: (identifier) @name) @declaration.method)
(enum_class_body
  (function_declaration name: (identifier) @name) @declaration.method)

(source_file
  (function_declaration name: (identifier) @name (function_body)) @definition.function)
(block
  (function_declaration name: (identifier) @name (function_body)) @definition.function)
(source_file
  (function_declaration name: (identifier) @name) @declaration.function)

; `fun Formatter.render()` and `val Formatter.tag` extend a receiver type:
; the receiver is an unlabelled child before the name, and `resolve_qualifier`
; drops any type arguments, so `List<T>` answers as `List`.
(source_file
  (function_declaration
    (user_type) @qualifier
    name: (identifier) @name
    (function_body)) @definition.method)
(source_file
  (property_declaration
    (user_type) @qualifier
    (variable_declaration (identifier) @name)) @definition.field)

; A property of a concrete holder declares storage whether or not it carries an
; initialiser or an accessor - `lateinit var service: Service` is a field just
; as much as `val code: Int = 400`. Only an `abstract` one merely declares.
; `const val` is a constant; any other member property is a field and a
; top-level one a variable.
((class_declaration "class"
  (class_body
    (property_declaration
      (variable_declaration (identifier) @name)) @definition.field))
  (#not-match? @definition.field "^[^=]*\\b(const|abstract)\\b"))
((class_declaration
  (enum_class_body
    (property_declaration
      (variable_declaration (identifier) @name)) @definition.field))
  (#not-match? @definition.field "^[^=]*\\b(const|abstract)\\b"))
((object_declaration
  (class_body
    (property_declaration
      (variable_declaration (identifier) @name)) @definition.field))
  (#not-match? @definition.field "^[^=]*\\b(const|abstract)\\b"))
((companion_object
  (class_body
    (property_declaration
      (variable_declaration (identifier) @name)) @definition.field))
  (#not-match? @definition.field "^[^=]*\\b(const|abstract)\\b"))
((object_literal
  (class_body
    (property_declaration
      (variable_declaration (identifier) @name)) @definition.field))
  (#not-match? @definition.field "^[^=]*\\b(const|abstract)\\b"))

((class_declaration "class"
  (class_body
    (property_declaration
      (modifiers (property_modifier) @_const)
      (variable_declaration (identifier) @name)) @definition.constant))
  (#eq? @_const "const"))
((class_declaration
  (enum_class_body
    (property_declaration
      (modifiers (property_modifier) @_const)
      (variable_declaration (identifier) @name)) @definition.constant))
  (#eq? @_const "const"))
((object_declaration
  (class_body
    (property_declaration
      (modifiers (property_modifier) @_const)
      (variable_declaration (identifier) @name)) @definition.constant))
  (#eq? @_const "const"))
((companion_object
  (class_body
    (property_declaration
      (modifiers (property_modifier) @_const)
      (variable_declaration (identifier) @name)) @definition.constant))
  (#eq? @_const "const"))
((object_literal
  (class_body
    (property_declaration
      (modifiers (property_modifier) @_const)
      (variable_declaration (identifier) @name)) @definition.constant))
  (#eq? @_const "const"))

(class_body
  (property_declaration
    (variable_declaration (identifier) @name)) @declaration.field)
(enum_class_body
  (property_declaration
    (variable_declaration (identifier) @name)) @declaration.field)

((source_file
  (property_declaration
    (variable_declaration (identifier) @name)) @definition.variable)
  (#not-match? @definition.variable "^[^=]*\\bconst\\b"))
((source_file
  (property_declaration
    (modifiers (property_modifier) @_const)
    (variable_declaration (identifier) @name)) @definition.constant)
  (#eq? @_const "const"))

; `class Point(val x: Int)` declares a property; a plain constructor parameter
; is a local and stays untagged. An `interface` has no primary constructor
; properties, so no interface carve-out is needed here.
((class_parameter (identifier) @name) @definition.field
  (#match? @definition.field "^[^=:]*\\b(val|var)\\b"))

; Calls

(call_expression (identifier) @name) @reference.call
(call_expression
  (navigation_expression
    (_) @qualifier
    (identifier) @name)) @reference.call
(callable_reference (identifier) @name) @reference.call

; Implementations
;
; A supertype list entry is either a bare type, a constructor invocation
; (`: RuntimeException()`) or a `by` delegation. Kotlin types are
; UpperCamelCase and package segments are lowercase, so the leading segments of
; a qualified name are filtered out by the case test.
((delegation_specifier
  (user_type (identifier) @name)) @reference.impl
  (#match? @name "^[A-Z]"))
((delegation_specifier
  (constructor_invocation
    (user_type (identifier) @name))) @reference.impl
  (#match? @name "^[A-Z]"))
((explicit_delegation
  (user_type (identifier) @name)) @reference.impl
  (#match? @name "^[A-Z]"))

; Imports

(import
  (qualified_identifier
    (identifier) @qualifier . (identifier) @name .)) @reference.import
(import
  (qualified_identifier . (identifier) @name .)) @reference.import
(import (qualified_identifier) (identifier) @name) @reference.import

; Other mentions

(navigation_expression
  (_) @qualifier
  (identifier) @name) @reference.field

; Every type mention in a signature - parameter, return, property, type
; argument, annotation, supertype - is a `user_type`.
((user_type (identifier) @name) @reference.type
  (#match? @name "^[A-Z]"))
((user_type
  (identifier) @qualifier . (identifier) @name) @reference.type
  (#match? @name "^[A-Z]"))
