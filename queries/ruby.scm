; Definitions

(class name: (constant) @name) @definition.class
(class
  name: (scope_resolution
    scope: (_) @qualifier
    name: (constant) @name)) @definition.class
(singleton_class value: (constant) @name) @definition.class
(module name: (constant) @name) @definition.module
(module
  name: (scope_resolution
    scope: (_) @qualifier
    name: (constant) @name)) @definition.module

(method name: (identifier) @name) @definition.method
(method name: (constant) @name) @definition.method
(method name: (setter (identifier) @name)) @definition.method
(singleton_method
  object: (_) @qualifier
  name: (identifier) @name) @definition.method
(singleton_method
  object: (_) @qualifier
  name: (setter (identifier) @name)) @definition.method
(alias name: (identifier) @name) @definition.method

; `attr_accessor :status` defines the reader (and the writer) it generates.
(call
  method: (identifier) @_attr
  arguments: (argument_list (simple_symbol) @name)
  (#any-of? @_attr "attr_accessor" "attr_reader" "attr_writer")) @definition.method

; `MAX = 8` at class or file level is Ruby's constant.
(assignment left: (constant) @name) @definition.constant
(assignment
  left: (scope_resolution
    scope: (_) @qualifier
    name: (constant) @name)) @definition.constant

; Calls
;
; Every Ruby message send is a call, including attribute reads: `obj.name`
; and `obj.name = x` both dispatch a method.

(call method: (identifier) @name) @reference.call
(call method: (constant) @name) @reference.call
(call
  receiver: (_) @qualifier
  method: (identifier) @name) @reference.call
(call
  receiver: (_) @qualifier
  method: (constant) @name) @reference.call

; Implementations

(superclass (constant) @name) @reference.impl
(superclass (scope_resolution name: (constant) @name)) @reference.impl

((call
  method: (identifier) @_mixin
  arguments: (argument_list
    [
      (constant) @name
      (scope_resolution name: (constant) @name)
    ])) @reference.impl
  (#any-of? @_mixin "include" "extend" "prepend"))

; Imports

((call
  method: (identifier) @_require
  arguments: (argument_list (string (string_content) @name))) @reference.import
  (#any-of? @_require "require" "require_relative" "autoload" "load"))

; Other mentions

(scope_resolution
  scope: (_) @qualifier
  name: (constant) @name) @reference.type

(constant) @name @reference.type
