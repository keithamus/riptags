; Definitions
;
; Top-level bindings only: a `function` node also spells a `where`/`let`
; binding, and those are locals.

(declarations
  (function name: (variable) @name)) @definition.function

; A nullary top-level binding (`limit = 128`, or any point-free definition)
; parses as `bind`, not `function`.
(declarations
  (bind name: (variable) @name)) @definition.function

(declarations
  (signature name: (variable) @name)) @declaration.function

(declarations
  (signature
    names: (binding_list name: (variable) @name))) @declaration.function

; User-defined operators: `(<+>) :: ...` and the infix definition head.

(declarations
  (signature name: (prefix_id (operator) @name))) @declaration.function

(declarations
  (function (infix operator: (operator) @name))) @definition.function

(data_type name: (name) @name) @definition.class
(newtype name: (name) @name) @definition.class
(class name: (name) @name) @definition.interface
(type_synomym name: (name) @name) @definition.type

; Families and their instances.

(data_family name: (name) @name) @definition.class
(type_family name: (name) @name) @definition.type
(type_instance name: (name) @name) @definition.type

; Data constructors live in the value namespace, so they are constants rather
; than a second `class` shadowing the type they build (`data Cache = Cache`
; would otherwise report two classes named `Cache`).

(data_constructor
  constructor: (prefix name: (constructor) @name)) @definition.constant

(data_constructor
  constructor: (record name: (constructor) @name)) @definition.constant

(gadt_constructor name: (constructor) @name) @definition.constant
(gadt_constructor
  names: (binding_list name: (constructor) @name)) @definition.constant

(newtype_constructor name: (constructor) @name) @definition.constant

(data_constructor
  constructor: (infix
    operator: (constructor_operator) @name)) @definition.constant

; Record fields.

(field
  name: (field_name (variable) @name)) @definition.field

; Class members and instance members: the enclosing `class`/`instance` scope
; rule supplies the owner.

(class_declarations
  (signature name: (variable) @name)) @declaration.method

(class_declarations
  (signature
    names: (binding_list name: (variable) @name))) @declaration.method

(class_declarations
  (function name: (variable) @name)) @definition.method

(instance_declarations
  (signature name: (variable) @name)) @declaration.method

(instance_declarations
  (function name: (variable) @name)) @definition.method

(class_declarations
  (bind name: (variable) @name)) @definition.method

(instance_declarations
  (bind name: (variable) @name)) @definition.method

; The module this file declares.

(header module: (module) @name) @definition.module

; Imports

(import module: (module) @name) @reference.import
(import alias: (module) @name) @reference.import

(import
  module: (module (module_id) @qualifier .)
  names: (import_list
    name: (import_name variable: (variable) @name))) @reference.import

(import
  module: (module (module_id) @qualifier .)
  names: (import_list
    name: (import_name type: (name) @name))) @reference.import

; Implementations

(instance name: (name) @name) @reference.impl
(instance patterns: (type_patterns (name) @name)) @reference.impl

; Calls: the head of an application is the function being called. A bare
; `variable` anywhere else is indistinguishable from a local read, so it stays
; untagged.

(apply function: (variable) @name) @reference.call

; An infix operator is unambiguously an applied function, unlike a bare
; `variable`, so its uses are callsites.
(infix operator: (operator) @name) @reference.call
(infix operator: (constructor_operator) @name) @reference.path

; In `qualified` position the `module` node swallows the trailing dot
; (`Map.`), which `resolve_qualifier` would split down to an empty segment, so
; the qualifier is the last `module_id` instead.

(apply
  function: (qualified
    module: (module (module_id) @qualifier .)
    id: (variable) @name)) @reference.call

; Other mentions

(qualified
  module: (module (module_id) @qualifier .)
  id: (variable) @name) @reference.path

(qualified
  module: (module (module_id) @qualifier .)
  id: (constructor) @name) @reference.path

(qualified
  module: (module (module_id) @qualifier .)
  id: (name) @name) @reference.type

(constructor) @name @reference.path

(name) @name @reference.type

; Record field use: construction, update, pattern match and projection.

(field_update field: (field_name (variable) @name)) @reference.field
(field_pattern field: (field_name (variable) @name)) @reference.field
(projection field: (field_name (variable) @name)) @reference.field
