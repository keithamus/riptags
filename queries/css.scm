; Definitions

; `.card {}` defines a class name that HTML `class="card"` then references.
(class_selector (class_name) @name) @definition.class
(id_selector (id_name) @name) @definition.constant

; `--brand: #0af` declares a custom property. `property_name` also covers every
; ordinary declaration (`color:`, `display:`), so the text predicate keeps the
; index to the custom ones - `color` is not a symbol anybody searches for.
(declaration
  (property_name) @name
  (#match? @name "^--")) @definition.variable

; `@property --brand { syntax: "<color>" }` registers the same kind of name;
; the grammar parses the name as the at-rule's `keyword_query`.
(at_rule
  (at_keyword) @_at
  (keyword_query) @name
  (#eq? @_at "@property")
  (#match? @name "^--")) @definition.variable

(keyframes_statement (keyframes_name) @name) @definition.constant

; References

; `var(--brand)` reads a custom property: a field read of the declared name.
; `arguments` is a comma-separated value list, so the name predicate is what
; keeps the fallback in `var(--brand, red)` out of the index.
(call_expression
  (function_name) @_fn
  (arguments (plain_value) @name)
  (#eq? @_fn "var")
  (#match? @name "^--")) @reference.field

; Every other function call. `var` is already recorded as the property read
; above, so it is the one name this pattern skips.
(call_expression
  (function_name) @name
  (#not-eq? @name "var")) @reference.call

; A tag selector mentions an element type, which is how `<my-widget>` in HTML
; and `my-widget {}` in CSS meet.
(tag_name) @name @reference.type

; Imports

(import_statement (string_value (string_content) @name)) @reference.import

(import_statement
  (call_expression
    (function_name) @_fn
    (arguments (string_value (string_content) @name)))
  (#eq? @_fn "url")) @reference.import
