#![cfg(all(feature = "css", feature = "html"))]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, named};

const CSS_SOURCE: &str = r#"
@import "base.css";

:root {
  --brand-color: #0af;
  color: red;
}

.card {
  color: var(--brand-color, red);
  width: calc(100% - 10px);
  background: url("card.png");
}

#main a {
  color: red;
}

@keyframes fade {
  from { opacity: 0; }
}

@property --ring-width {
  syntax: "<length>";
}
"#;

const HTML_SOURCE: &str = r#"
<div id="main" class="card">
  <a href="/docs">docs</a>
  <my-widget data-size="2"></my-widget>
  <input id=search class="field wide"/>
</div>
"#;

#[test]
fn css_tags_selectors_custom_properties_and_calls() {
    let tags = tag_source("web/app.css", CSS_SOURCE);

    let classes = find(&tags, "card", OccKind::Definition);
    assert_eq!(classes.len(), 1, "one `.card` definition, got {tags:#?}");
    assert_eq!(classes[0].sym_kind, SymKind::Class);
    assert_eq!(classes[0].line, 9);
    assert_eq!(classes[0].col, 1, "the name starts after the dot");
    assert!(classes[0].owners.is_empty(), "css has no owning scopes");

    let ids = find(&tags, "main", OccKind::Definition);
    assert_eq!(ids.len(), 1, "`#main` selector defines the id");
    assert_eq!(ids[0].sym_kind, SymKind::Constant);

    // A custom property declaration and its `var()` read share one name, so
    // `rt --brand-color` answers with the declaration plus every use.
    let prop_defs = find(&tags, "--brand-color", OccKind::Definition);
    assert_eq!(prop_defs.len(), 1, "`--brand-color:` is a definition");
    assert_eq!(prop_defs[0].sym_kind, SymKind::Variable);
    assert_eq!(prop_defs[0].line, 5);
    let prop_uses = find(&tags, "--brand-color", OccKind::Field);
    assert_eq!(prop_uses.len(), 1, "`var(--brand-color)` is a read");
    assert_eq!(prop_uses[0].line, 10);

    let registered = find(&tags, "--ring-width", OccKind::Definition);
    assert_eq!(
        registered.len(),
        1,
        "`@property --ring-width` defines a name"
    );
    assert_eq!(registered[0].sym_kind, SymKind::Variable);

    let keyframes = find(&tags, "fade", OccKind::Definition);
    assert_eq!(keyframes.len(), 1, "`@keyframes fade` defines a name");

    assert_eq!(
        find(&tags, "calc", OccKind::Call).len(),
        1,
        "calc() is a call"
    );
    assert_eq!(
        find(&tags, "url", OccKind::Call).len(),
        1,
        "url() is a call like any other function"
    );
    assert_eq!(
        find(&tags, "a", OccKind::Type).len(),
        1,
        "the `#main a` tag selector mentions an element type"
    );
    assert_eq!(
        find(&tags, "base.css", OccKind::Import).len(),
        1,
        "@import records its target"
    );

    // Noise that must stay out of the index.
    assert!(
        named(&tags, "color").is_empty(),
        "ordinary properties are not symbols"
    );
    assert!(named(&tags, "var").is_empty(), "var() is a property read");
    assert!(
        named(&tags, "red").is_empty(),
        "the `var(--brand-color, red)` fallback is a value, not a symbol"
    );
    assert!(named(&tags, "root").is_empty(), "`:root` is a pseudo class");

    // Cross-file: a property declared in one stylesheet and read in another
    // folds into a single index symbol.
    let read = tag_source("web/other.css", ".a { margin: var(--ring-width); }\n");
    assert_eq!(
        registered[0].symbols(),
        find(&read, "--ring-width", OccKind::Field)[0].symbols()
    );
}

#[test]
fn html_tags_element_ids_and_classes() {
    let tags = tag_source("web/index.html", HTML_SOURCE);

    let ids = find(&tags, "main", OccKind::Definition);
    assert_eq!(ids.len(), 1, "`id=\"main\"` defines the id, got {tags:#?}");
    assert_eq!(ids[0].sym_kind, SymKind::Constant);
    assert_eq!(ids[0].line, 2);
    assert!(ids[0].owners.is_empty(), "html has no owning scopes");

    let unquoted = find(&tags, "search", OccKind::Definition);
    assert_eq!(unquoted.len(), 1, "an unquoted `id=search` counts too");

    let classes = find(&tags, "card", OccKind::Path);
    assert_eq!(
        classes.len(),
        1,
        "`class=\"card\"` references the css class"
    );
    assert_eq!(classes[0].line, 2);
    assert!(
        find(&tags, "card", OccKind::Definition).is_empty(),
        "html uses classes, css defines them"
    );

    let custom = find(&tags, "my-widget", OccKind::Type);
    assert_eq!(custom.len(), 1, "custom element usage is searchable");
    assert_eq!(custom[0].sym_kind, SymKind::Type);

    // Only `id` and `class` are selected; every other attribute is noise.
    assert!(named(&tags, "href").is_empty(), "attribute names untagged");
    assert!(named(&tags, "/docs").is_empty(), "href values untagged");
    assert!(named(&tags, "data-size").is_empty(), "data-* untagged");
    assert!(named(&tags, "2").is_empty(), "attribute values untagged");

    // A multi-class value is one grammar token, so it is tagged whole.
    assert_eq!(find(&tags, "field wide", OccKind::Path).len(), 1);
    assert!(named(&tags, "field").is_empty(), "values are not split");

    // The payoff: `.card {}` in css and `class="card"` here are one symbol.
    let css = tag_source("web/app.css", ".card { color: red; }\n");
    let def = find(&css, "card", OccKind::Definition);
    assert_eq!(def.len(), 1);
    assert_eq!(
        def[0].symbols(),
        classes[0].symbols(),
        "both files must fold into one index symbol"
    );
}
