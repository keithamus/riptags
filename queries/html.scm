; Definitions

; `id="main"` names this element; a CSS `#main` selector defines the same name,
; so both sides of the pair answer one query.
(attribute
  (attribute_name) @_attr
  [
    (quoted_attribute_value (attribute_value) @name)
    (attribute_value) @name
  ]
  (#eq? @_attr "id")) @definition.constant

; References

; Element usage. Anchored on `element` so `script`/`style` tags stay out of the
; index; the payoff is custom elements (`<my-widget>`).
(element (start_tag (tag_name) @name)) @reference.type
(element (self_closing_tag (tag_name) @name)) @reference.type

; `class="card"` uses the CSS class `.card`, so it pairs with CSS
; `@definition.class` (the tagger has no dedicated class-reference kind and
; folds this into a generic path reference - `@reference.type` stays reserved
; for element names). The grammar makes the whole attribute value one token,
; so `class="a b"` is tagged as the single name `a b`.
(attribute
  (attribute_name) @_attr
  [
    (quoted_attribute_value (attribute_value) @name)
    (attribute_value) @name
  ]
  (#eq? @_attr "class")) @reference.class
