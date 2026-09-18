; Definitions

(function_definition name: (identifier) @name) @definition.function
(class_definition name: (identifier) @name) @definition.class

; A `def` in a class body is a method.

(class_definition
  body: (block
    (function_definition name: (identifier) @name) @definition.method))
(class_definition
  body: (block
    (decorated_definition
      (function_definition name: (identifier) @name)) @definition.method))

; Assignments: a class body declares fields, `self.x = ...` adds one, a
; capitalised receiver patches the class it names, and the module level
; declares constants (uppercase) or variables.

(class_definition
  body: (block
    (expression_statement
      (assignment left: (identifier) @name) @definition.field)))

(assignment
  left: (attribute
    object: (identifier) @qualifier
    attribute: (identifier) @name)
  (#match? @qualifier "^(self|[A-Z])")) @definition.field

(module
  (expression_statement
    (assignment left: (identifier) @name) @definition.variable))

((module
  (expression_statement
    (assignment left: (identifier) @name) @definition.constant))
  (#match? @name "^[A-Z][A-Z0-9_]*$"))

(assignment
  left: (identifier) @name
  right: (lambda)) @definition.function

; Calls

(call function: (identifier) @name) @reference.call

(call
  function: (attribute
    object: (_) @qualifier
    attribute: (identifier) @name)) @reference.call

(decorator (identifier) @name) @reference.call

; Imports

(import_statement name: (dotted_name (identifier) @name .)) @reference.import

(import_from_statement
  module_name: (dotted_name) @qualifier
  name: (dotted_name (identifier) @name .)) @reference.import

(import_statement
  name: (aliased_import alias: (identifier) @name)) @reference.import

(import_from_statement
  module_name: (dotted_name) @qualifier
  name: (aliased_import alias: (identifier) @name)) @reference.import

; Other mentions

(attribute
  object: (_) @qualifier
  attribute: (identifier) @name) @reference.field
