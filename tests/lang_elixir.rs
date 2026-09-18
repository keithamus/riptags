#![cfg(feature = "elixir")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const ELIXIR_SOURCE: &str = r#"
defmodule Store.Cache do
  @moduledoc "cache"
  @default_ttl 300

  alias Store.Entry
  alias Store.{Index, Meta}
  import Logger, only: [info: 1]
  require Logger
  use GenServer

  defstruct [:name, :ttl]

  @spec fetch(String.t()) :: Entry.t()
  defmacro trace(expr) do
    quote do: unquote(expr)
  end

  def fetch(key) do
    Entry.load(key)
    |> normalize()
  end

  def put(key, value) do
    entry = %Entry{name: key, ttl: @default_ttl}
    Logger.info("put")
    :ets.insert(:cache, entry)
    Index.add(Meta.wrap(entry))
    Enum.each([key], &normalize/1)
  end

  defp normalize(key), do: to_string(key)

  defguard is_valid(key) when is_binary(key)

  def ready?, do: true
end

defprotocol Sizable do
  def size(value)
end

defimpl Sizable, for: Map do
  def size(map), do: map_size(map)
end
"#;

/// Elixir spells every declaration as a call, so `ELIXIR_SCOPES` keys its
/// scopes on the called target: a `defmodule` body owns what it defines and
/// a `def` names the context of the references in its body.
#[test]
fn elixir_defs_and_references() {
    let tags = tag_source("lib/store/cache.ex", ELIXIR_SOURCE);

    // defmodule -> module. The alias token is the whole dotted path.
    let module = find(&tags, "Store.Cache", OccKind::Definition);
    assert_eq!(module.len(), 1);
    assert_eq!(module[0].sym_kind, SymKind::Module);
    assert_eq!(module[0].line, 2);
    assert_eq!(module[0].col, 10);
    assert!(
        owners(module[0]).is_empty(),
        "the module does not own itself"
    );
    assert_eq!(module[0].context, None);

    // defprotocol -> interface, not module.
    let protocol = find(&tags, "Sizable", OccKind::Definition);
    assert_eq!(protocol.len(), 1);
    assert_eq!(protocol[0].sym_kind, SymKind::Interface);

    // def / defp -> function.
    let fetch = find(&tags, "fetch", OccKind::Definition);
    assert_eq!(fetch.len(), 1);
    assert_eq!(fetch[0].sym_kind, SymKind::Function);
    assert_eq!(fetch[0].line, 19);
    assert_eq!(
        owners(fetch[0]),
        vec!["Store.Cache"],
        "the enclosing defmodule owns its functions"
    );
    assert_eq!(fetch[0].context.as_deref(), Some("Store.Cache"));
    let normalize_def = find(&tags, "normalize", OccKind::Definition);
    assert_eq!(normalize_def.len(), 1);
    assert_eq!(normalize_def[0].sym_kind, SymKind::Function);
    // zero-arity clause written without parentheses, trailing `?` kept
    let ready = find(&tags, "ready?", OccKind::Definition);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].sym_kind, SymKind::Function);
    assert_eq!(ready[0].len, 6);

    // defmacro / defguard -> macro.
    let trace = find(&tags, "trace", OccKind::Definition);
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].sym_kind, SymKind::Macro);
    let guard = find(&tags, "is_valid", OccKind::Definition);
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].sym_kind, SymKind::Macro);

    // A module attribute with a value is a constant; its read is a field use.
    let ttl = find(&tags, "default_ttl", OccKind::Definition);
    assert_eq!(ttl.len(), 1);
    assert_eq!(ttl[0].sym_kind, SymKind::Constant);
    assert_eq!(find(&tags, "default_ttl", OccKind::Field).len(), 1);

    // Remote call: the capitalised receiver resolves to the owning module.
    let load = find(&tags, "load", OccKind::Call);
    assert_eq!(load.len(), 1);
    assert_eq!(owners(load[0]), vec!["Entry"]);
    assert_eq!(
        load[0].context.as_deref(),
        Some("Store.Cache::fetch"),
        "a call inside a function names the module and the function"
    );
    let info = find(&tags, "info", OccKind::Call);
    assert_eq!(info.len(), 1);
    assert_eq!(owners(info[0]), vec!["Logger"]);
    // Nested remote call inside another call's arguments.
    let wrap = find(&tags, "wrap", OccKind::Call);
    assert_eq!(wrap.len(), 1);
    assert_eq!(owners(wrap[0]), vec!["Meta"]);
    // An erlang module is an atom, not an alias: no owner is claimed.
    let insert = find(&tags, "insert", OccKind::Call);
    assert_eq!(insert.len(), 1);
    assert!(owners(insert[0]).is_empty());

    // Local calls: plain, piped without parentheses, and `&name/arity`.
    let to_string = find(&tags, "to_string", OccKind::Call);
    assert_eq!(to_string.len(), 1);
    assert!(owners(to_string[0]).is_empty());
    let normalize_call = find(&tags, "normalize", OccKind::Call);
    assert_eq!(
        normalize_call
            .iter()
            .map(|tag| tag.line)
            .collect::<Vec<_>>(),
        vec![21, 29],
        "`|> normalize()` and `&normalize/1`"
    );

    // alias / import / require / use are imports.
    let entry_alias = find(&tags, "Store.Entry", OccKind::Import);
    assert_eq!(entry_alias.len(), 1);
    assert_eq!(entry_alias[0].line, 6);
    let logger = find(&tags, "Logger", OccKind::Import);
    assert_eq!(logger.len(), 2, "import Logger and require Logger");
    assert_eq!(find(&tags, "GenServer", OccKind::Import).len(), 1);

    // defimpl is an implementation reference owned by the target module.
    let impl_ref = find(&tags, "Sizable", OccKind::Impl);
    assert_eq!(impl_ref.len(), 1);
    assert_eq!(owners(impl_ref[0]), vec!["Map"]);

    // Every other alias is a module mention: struct literals, receivers,
    // typespec module names, the members of a multi-alias.
    let entry_types = find(&tags, "Entry", OccKind::Type);
    assert_eq!(
        entry_types.len(),
        3,
        "Entry.t() in the spec, Entry.load receiver, %Entry{{}} literal"
    );
    assert!(
        entry_types.iter().any(|tag| tag.line == 25),
        "the %Entry{{}} literal is a module mention"
    );
    assert_eq!(find(&tags, "Index", OccKind::Type).len(), 2);
    assert_eq!(find(&tags, "Map", OccKind::Type).len(), 1);

    // Deliberately untagged: `defstruct` (the struct is the module, already
    // tagged), reserved attributes, and Kernel special forms.
    for noise in ["moduledoc", "spec", "defstruct", "quote", "unquote"] {
        assert!(
            tags.iter().all(|tag| &*tag.name != noise),
            "{noise} should not be tagged"
        );
    }
}
