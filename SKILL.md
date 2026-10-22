---
name: riptags
description: "`rt` finds where a symbol is defined and every callsite. Use it instead of grep/rg when the search term is an identifier - function, method, class, struct, constant, field - in any mainstream language."
---

# riptags (rt)

`rt <symbol>` prints the definition and every callsite from a tree-sitter
index, ranked, each labelled with its enclosing symbol. The index builds and
refreshes itself.

```bash
rt                        # tree summary: files, languages, kinds
rt send_request           # definitions and callsites
rt Client::send_request   # one owner only
rt send_request --refs    # callsites only (--def for definitions)
rt send --search          # names containing "send"
rt --class                # every class, struct and enum
```

Sites print as `path:line:col [enclosing] source`; exit 1 means no such
symbol. Flags follow ripgrep (`-g <GLOB>`, `-t rust`, `-i`, `-C <N>`, `-m <N>`,
`-l`); `rt --help` lists the rest.

- Query names only, not a regex, glob or path.
- Trust a miss; it rechecks the tree first. Don't re-grep listed callsites.
- Use `rg` for non-identifiers (log strings, comments, config) and languages
  `rt` can't parse.
- Never run `--index`/`--watch` first, or touch the index cache.
