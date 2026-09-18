; Elixir has no dedicated declaration nodes: `defmodule`, `def`, `alias` and a
; user-defined macro all parse to the same `(call target: (identifier) ...)`.
; The only way to tell them apart is a text predicate on the target, so every
; definition pattern below is a `#eq?`/`#any-of?` on that identifier.

; Definitions

; * defmodule Store.Cache do
(call
  target: (identifier) @_form
  (arguments . (alias) @name)
  (#eq? @_form "defmodule")) @definition.module

; * defprotocol Sizable do  -- a protocol is Elixir's interface
(call
  target: (identifier) @_form
  (arguments . (alias) @name)
  (#eq? @_form "defprotocol")) @definition.interface

; * def fetch(key) / defp normalize(key) / def ready? / def size(v) when ...
(call
  target: (identifier) @_form
  (arguments
    .
    [
      ; zero-arity clause written without parentheses
      (identifier) @name
      ; regular clause
      (call target: (identifier) @name)
      ; clause with a guard
      (binary_operator
        left: [
          (identifier) @name
          (call target: (identifier) @name)
        ]
        operator: "when")
    ])
  (#any-of? @_form "def" "defp" "defdelegate" "defn" "defnp")) @definition.function

; * defmacro trace(expr) / defguard is_valid(key) when ...
(call
  target: (identifier) @_form
  (arguments
    .
    [
      (identifier) @name
      (call target: (identifier) @name)
      (binary_operator
        left: [
          (identifier) @name
          (call target: (identifier) @name)
        ]
        operator: "when")
    ])
  (#any-of? @_form "defmacro" "defmacrop" "defguard" "defguardp")) @definition.macro

; * @timeout 5_000 -- a module attribute carrying a value is a module constant.
;   The reserved documentation/typespec attributes are skipped: `@doc`,
;   `@spec` and friends appear on nearly every function and say nothing about
;   a symbol named `doc` or `spec`.
(unary_operator
  operator: "@"
  operand: (call target: (identifier) @name)
  (#not-any-of? @name
    "doc" "moduledoc" "typedoc" "spec" "type" "typep" "opaque" "callback"
    "macrocallback" "behaviour" "behavior" "impl" "deprecated" "derive"
    "enforce_keys" "optional_callbacks" "fallback_to_any" "compile"
    "dialyzer" "external_resource" "after_compile" "before_compile"
    "on_definition" "on_load" "vsn")) @definition.constant

; References

; * alias Store.Entry / alias Store.{Index, Meta} / import Logger / use GenServer
(call
  target: (identifier) @_form
  (arguments
    .
    [
      (alias) @name
      (dot left: (alias) @name)
    ])
  (#any-of? @_form "alias" "import" "require" "use")) @reference.import

; * defimpl Sizable, for: Map -- the implemented protocol, owned by the target
(call
  target: (identifier) @_form
  (arguments . (alias) @name)
  (#eq? @_form "defimpl")) @reference.impl

(call
  target: (identifier) @_form
  (arguments
    .
    (alias) @name
    (keywords (pair value: (alias) @qualifier)))
  (#eq? @_form "defimpl")) @reference.impl

; * local call: to_string(key). Every special form and def-macro is filtered
;   out by name, because the grammar gives them the same shape as a real call.
(call
  target: (identifier) @name
  (#not-any-of? @name
    "def" "defp" "defmacro" "defmacrop" "defguard" "defguardp" "defdelegate"
    "defn" "defnp" "defmodule" "defprotocol" "defimpl" "defstruct"
    "defexception" "defoverridable" "defrecord" "defrecordp"
    "alias" "import" "require" "use"
    "case" "cond" "for" "if" "unless" "with" "try" "receive" "quote"
    "unquote" "unquote_splicing" "raise" "reraise" "throw" "super" "fn"
    "doc" "moduledoc" "typedoc" "spec" "type" "typep" "opaque" "callback"
    "macrocallback" "behaviour" "behavior" "impl" "deprecated" "derive"
    "enforce_keys" "optional_callbacks" "fallback_to_any" "compile"
    "dialyzer" "external_resource" "after_compile" "before_compile"
    "on_definition" "on_load" "vsn")) @reference.call

; * remote call: Entry.load(key), :ets.insert(...). A capitalised receiver is
;   a module and becomes the owner; a lowercase one is a variable and does not.
;   Parenthesis-free member access (`conn.assigns`, `Stream.run`) parses as a
;   `call` with no arguments too, so it lands here rather than as a field use:
;   the grammar genuinely cannot tell a map key from a zero-arity remote call.
(call
  target: (dot
    left: (_) @qualifier
    right: (identifier) @name)) @reference.call

; * piped call written without parentheses: value |> normalize
;   (`value |> Entry.load` is already a `call`, matched above)
(binary_operator
  operator: "|>"
  right: (identifier) @name) @reference.call

; * function capture of a local function: &normalize/1
;   (`&Entry.load/1` is a `call`, matched above)
(unary_operator
  operator: "&"
  operand: (binary_operator
    left: (identifier) @name
    operator: "/"
    right: (integer))) @reference.call

; * every module mention: alias paths, `%Entry{}` struct literals, the
;   receiver of a remote call, module names in typespecs.
(alias) @name @reference.type

; * module attribute read: @default_ttl
(unary_operator
  operator: "@"
  operand: (identifier) @name) @reference.field
