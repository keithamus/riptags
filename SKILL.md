---
name: riptags
description: "`rt` finds where a symbol is defined and every callsite. Use it instead of grep/rg when the search term is an identifier - function, method, class, struct, constant, field - in any mainstream language."
---

# riptags (rt)

`rt <symbol>` answers from a tree-sitter index of the tree: the definition and
every callsite, ranked, each site labelled with the symbol enclosing it. Tens
of lines where `rg` on a popular name gives thousands. Nothing to set up - the
index builds and refreshes itself.

```bash
rt                        # what this tree holds: files, symbols, languages, kinds
rt send_request           # definitions and callsites
rt Client::send_request   # narrowed to one owner; bare name matches any
rt send_request --refs    # callsites only, --def for the other half
rt send --search          # symbol names containing "send"
rt --class                # every class, struct and enum in the tree
```

Sites read `path:line:col [enclosing] source`, grouped `def` `decl` `impl`
`ref`; exit 1 means no such symbol. `rt --help` is one screen and has the rest:
the other kinds, `--limit`, `--glob`, `--type`, `--json`, `--fresh`,
another tree, and `--vimgrep`/`-l` when the answer is going through a pipe.
Flags follow ripgrep: `-d`/`--def`, `-r`/`--refs`, `-m <N>`, `-g <GLOB>`,
`-t rust`, `-i`, `-C <N>`, `-q`, and `--func`, `--iface`, `--mod`, `--const`,
`--var` for the kinds.

**Do**

- Query a plain name; narrow with `Owner::name`, `-g <GLOB>`, `-t rust`,
  or a `[ROOT]` argument.
- Trust an empty answer: a miss rechecks the tree before reporting it.
- Fall back to `rg` for what is not an identifier - log strings, comments,
  config values, and languages `rt` cannot parse.

**Don't**

- Pass a regex, a glob or a path as the query; names only.
- Run `--index` or `--watch` first - a query indexes what it needs.
- Re-grep the tree for callsites `rt` has already listed.
- Remove index caches, or otherwise manually inspect them.
