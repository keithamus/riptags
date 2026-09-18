#![cfg(all(feature = "csharp", feature = "php", feature = "ruby"))]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const CSHARP_SOURCE: &str = r#"
using System.Text;

namespace Shop
{
    public interface IRepo
    {
        Order Find(int id);
        int Count { get; }
    }

    public class OrderRepo : IRepo
    {
        private const int MaxRetries = 3;
        private readonly StringBuilder _log = new StringBuilder();

        public int Count { get; set; }

        public event EventHandler Changed
        {
            add { }
            remove { }
        }

        public OrderRepo()
        {
            Warm();
        }

        public Order Find(int id)
        {
            int Scale(int n) => n * 2;
            _log.Clear();
            return Order.Parse(id, this.Count);
        }
    }
}
"#;

#[test]
fn csharp_members_are_owned_by_their_type() {
    let tags = tag_source("Shop/OrderRepo.cs", CSHARP_SOURCE);

    let repo = find(&tags, "OrderRepo", OccKind::Definition);
    assert_eq!(repo.len(), 2, "the class and its constructor");
    let class = repo
        .iter()
        .find(|tag| tag.sym_kind == SymKind::Class)
        .expect("the class definition");
    assert_eq!(class.context.as_deref(), Some("Shop"));

    let irepo = find(&tags, "IRepo", OccKind::Definition);
    assert_eq!(irepo.len(), 1);
    assert_eq!(irepo[0].sym_kind, SymKind::Interface);

    let find_def = find(&tags, "Find", OccKind::Definition);
    assert_eq!(find_def.len(), 1);
    assert_eq!(find_def[0].sym_kind, SymKind::Method);
    assert_eq!(owners(find_def[0]), vec!["OrderRepo"]);
    assert_eq!(find_def[0].context.as_deref(), Some("Shop::OrderRepo"));

    let find_decl = find(&tags, "Find", OccKind::Declaration);
    assert_eq!(find_decl.len(), 1, "the interface member only declares");
    assert_eq!(owners(find_decl[0]), vec!["IRepo"]);

    let max = find(&tags, "MaxRetries", OccKind::Definition);
    assert_eq!(max.len(), 1);
    assert_eq!(max[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(max[0]), vec!["OrderRepo"]);

    let log = find(&tags, "_log", OccKind::Definition);
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].sym_kind, SymKind::Field);

    let count_def = find(&tags, "Count", OccKind::Definition);
    assert_eq!(count_def.len(), 1, "the auto-property is a definition");
    assert_eq!(count_def[0].sym_kind, SymKind::Field);
    assert_eq!(owners(count_def[0]), vec!["OrderRepo"]);
    let count_decl = find(&tags, "Count", OccKind::Declaration);
    assert_eq!(count_decl.len(), 1, "the interface property only declares");
    assert_eq!(owners(count_decl[0]), vec!["IRepo"]);

    let parse = find(&tags, "Parse", OccKind::Call);
    assert_eq!(parse.len(), 1);
    assert_eq!(
        owners(parse[0]),
        vec!["Order"],
        "`Order.Parse(...)` resolves its owner through @qualifier"
    );
    assert_eq!(parse[0].context.as_deref(), Some("Shop::OrderRepo::Find"));

    let clear = find(&tags, "Clear", OccKind::Call);
    assert_eq!(clear.len(), 1);
    assert!(
        clear[0].owners.is_empty(),
        "`_log` is not a type name, so no owner is claimed"
    );

    let builder = find(&tags, "StringBuilder", OccKind::Call);
    assert_eq!(
        builder.len(),
        1,
        "`new StringBuilder()` is an instantiation"
    );

    let count_read = find(&tags, "Count", OccKind::Field);
    assert_eq!(count_read.len(), 1);
    assert_eq!(
        owners(count_read[0]),
        vec!["OrderRepo"],
        "`this.Count` resolves to the enclosing class"
    );

    let base = find(&tags, "IRepo", OccKind::Impl);
    assert_eq!(base.len(), 1, "the base list is an implementation");

    let import = find(&tags, "Text", OccKind::Import);
    assert_eq!(import.len(), 1);
    assert_eq!(owners(import[0]), vec!["System"]);

    let order_type = find(&tags, "Order", OccKind::Type);
    assert!(
        !order_type.is_empty(),
        "`Order Find(int id)` mentions the return type"
    );

    let ctor = repo
        .iter()
        .find(|tag| tag.sym_kind == SymKind::Method)
        .expect("the constructor definition");
    assert_eq!(owners(ctor), vec!["OrderRepo"]);

    let changed = find(&tags, "Changed", OccKind::Definition);
    assert_eq!(changed.len(), 1, "an event with accessors defines");
    assert_eq!(owners(changed[0]), vec!["OrderRepo"]);

    let warm = find(&tags, "Warm", OccKind::Call);
    assert_eq!(warm.len(), 1);
    assert_eq!(
        warm[0].context.as_deref(),
        Some("Shop::OrderRepo::OrderRepo"),
        "a call inside a constructor names it"
    );

    let scale = find(&tags, "Scale", OccKind::Definition);
    assert_eq!(scale.len(), 1, "a local function is still a definition");
    assert_eq!(scale[0].context.as_deref(), Some("Shop::OrderRepo::Find"));
}

const PHP_SOURCE: &str = r#"<?php

namespace App\Repo;

use App\Model\Order;

require_once 'bootstrap.php';

interface RepoInterface
{
    public function find(int $id): ?Order;
}

class OrderRepo implements RepoInterface
{
    public const MAX_RETRIES = 3;

    private array $cache = [];

    public function __construct(private int $shard, private readonly string $dsn)
    {
    }

    public function find(int $id): ?Order
    {
        $this->warm();
        self::warm();
        static::warm();
        parent::warm();
        return Order::parse($id);
    }

    private function warm(): void
    {
    }
}

$normalize = function (string $s): string { return \trim($s); };
$double = fn($x) => $x * 2;
"#;

#[test]
fn php_members_are_owned_by_their_class() {
    let tags = tag_source("src/Repo/OrderRepo.php", PHP_SOURCE);

    let repo = find(&tags, "OrderRepo", OccKind::Definition);
    assert_eq!(repo.len(), 1);
    assert_eq!(repo[0].sym_kind, SymKind::Class);
    assert_eq!(
        repo[0].context.as_deref(),
        None,
        "`namespace App\\Repo;` has no body, so the class is its sibling, not \
         its child - only the braced form nests"
    );

    let iface = find(&tags, "RepoInterface", OccKind::Definition);
    assert_eq!(iface.len(), 1);
    assert_eq!(iface[0].sym_kind, SymKind::Interface);

    let find_def = find(&tags, "find", OccKind::Definition);
    assert_eq!(find_def.len(), 1);
    assert_eq!(find_def[0].sym_kind, SymKind::Method);
    assert_eq!(owners(find_def[0]), vec!["OrderRepo"]);
    assert_eq!(find_def[0].context.as_deref(), Some("OrderRepo"));

    let find_decl = find(&tags, "find", OccKind::Declaration);
    assert_eq!(find_decl.len(), 1, "the interface method has no body");
    assert_eq!(owners(find_decl[0]), vec!["RepoInterface"]);

    let max = find(&tags, "MAX_RETRIES", OccKind::Definition);
    assert_eq!(max.len(), 1);
    assert_eq!(max[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(max[0]), vec!["OrderRepo"]);

    let cache = find(&tags, "cache", OccKind::Definition);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache[0].sym_kind, SymKind::Field);
    assert_eq!(owners(cache[0]), vec!["OrderRepo"]);

    let warm = find(&tags, "warm", OccKind::Call);
    assert_eq!(
        warm.len(),
        4,
        "`$this->`, `self::`, `static::` and `parent::` are all callsites"
    );
    assert_eq!(
        warm.iter()
            .flat_map(|call| owners(call))
            .collect::<Vec<_>>(),
        vec!["OrderRepo"; 3],
        "the first three resolve to the enclosing class; `parent::` names a \
         class this file does not, so it claims no owner"
    );
    for call in &warm {
        assert_eq!(
            call.context.as_deref(),
            Some("OrderRepo::find"),
            "a semicolon namespace does not enclose the class it declares"
        );
    }

    let parse = find(&tags, "parse", OccKind::Call);
    assert_eq!(parse.len(), 1);
    assert_eq!(
        owners(parse[0]),
        vec!["Order"],
        "`Order::parse()` resolves its owner through @qualifier"
    );

    let impl_ref = find(&tags, "RepoInterface", OccKind::Impl);
    assert_eq!(impl_ref.len(), 1);

    let import = find(&tags, "Order", OccKind::Import);
    assert_eq!(import.len(), 1);
    assert_eq!(owners(import[0]), vec!["Model"]);

    let required = find(&tags, "bootstrap.php", OccKind::Import);
    assert_eq!(required.len(), 1, "`require_once` names a file");

    let order_type = find(&tags, "Order", OccKind::Type);
    assert_eq!(order_type.len(), 2, "both `?Order` return types");

    for (name, owner) in [("shard", "OrderRepo"), ("dsn", "OrderRepo")] {
        let promoted = find(&tags, name, OccKind::Definition);
        assert_eq!(promoted.len(), 1, "promoted constructor property {name}");
        assert_eq!(promoted[0].sym_kind, SymKind::Field);
        assert_eq!(owners(promoted[0]), vec![owner]);
    }

    for name in ["normalize", "double"] {
        let bound = find(&tags, name, OccKind::Definition);
        assert_eq!(bound.len(), 1, "`${name} = fn/function` binds a callable");
        assert_eq!(bound[0].sym_kind, SymKind::Function);
    }

    let trim = find(&tags, "trim", OccKind::Call);
    assert_eq!(trim.len(), 1, "`\\trim()` calls the root namespace");
}

const RUBY_SOURCE: &str = r#"
require "json"

module Shop
  class OrderRepo < BaseRepo
    include Loggable

    MAX_RETRIES = 3

    attr_accessor :status
    attr_reader :total

    def find(id)
      warm
      refresh()
      Order.parse(id)
    end

    def self.build
      new
    end
  end
end

class << Logger
  def flush
  end
end
"#;

#[test]
fn ruby_methods_are_owned_by_their_class() {
    let tags = tag_source("app/repo/order_repo.rb", RUBY_SOURCE);

    let shop = find(&tags, "Shop", OccKind::Definition);
    assert_eq!(shop.len(), 1);
    assert_eq!(shop[0].sym_kind, SymKind::Module);

    let repo = find(&tags, "OrderRepo", OccKind::Definition);
    assert_eq!(repo.len(), 1);
    assert_eq!(repo[0].sym_kind, SymKind::Class);
    assert_eq!(repo[0].context.as_deref(), Some("Shop"));

    let find_def = find(&tags, "find", OccKind::Definition);
    assert_eq!(find_def.len(), 1);
    assert_eq!(find_def[0].sym_kind, SymKind::Method);
    assert_eq!(owners(find_def[0]), vec!["OrderRepo"]);
    assert_eq!(find_def[0].context.as_deref(), Some("Shop::OrderRepo"));

    let build = find(&tags, "build", OccKind::Definition);
    assert_eq!(build.len(), 1, "`def self.build` is a definition");
    assert_eq!(owners(build[0]), vec!["OrderRepo"]);

    let max = find(&tags, "MAX_RETRIES", OccKind::Definition);
    assert_eq!(max.len(), 1);
    assert_eq!(max[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(max[0]), vec!["OrderRepo"]);

    let parse = find(&tags, "parse", OccKind::Call);
    assert_eq!(parse.len(), 1);
    assert_eq!(
        owners(parse[0]),
        vec!["Order"],
        "`Order.parse(id)` resolves its owner through @qualifier"
    );
    assert_eq!(parse[0].context.as_deref(), Some("Shop::OrderRepo::find"));

    let refresh = find(&tags, "refresh", OccKind::Call);
    assert_eq!(refresh.len(), 1, "`refresh()` is a call");
    assert!(refresh[0].owners.is_empty());
    assert_eq!(refresh[0].context.as_deref(), Some("Shop::OrderRepo::find"));

    assert!(
        find(&tags, "warm", OccKind::Call).is_empty(),
        "a bare `warm` parses as an identifier, indistinguishable from a local \
         variable read, so tagging it would mean tagging every local"
    );

    let base = find(&tags, "BaseRepo", OccKind::Impl);
    assert_eq!(base.len(), 1, "the superclass is an implementation");

    let mixin = find(&tags, "Loggable", OccKind::Impl);
    assert_eq!(mixin.len(), 1, "`include Loggable` is an implementation");

    let import = find(&tags, "json", OccKind::Import);
    assert_eq!(import.len(), 1, "`require \"json\"` names its file");

    // `attr_accessor :status` defines the accessors it generates.
    for name in ["status", "total"] {
        let attr = find(&tags, name, OccKind::Definition);
        assert_eq!(attr.len(), 1, "{name} is generated by attr_*");
        assert_eq!(attr[0].sym_kind, SymKind::Method);
        assert_eq!(owners(attr[0]), vec!["OrderRepo"]);
    }

    // `class << Logger` reopens a type, so its methods belong to it.
    let flush = find(&tags, "flush", OccKind::Definition);
    assert_eq!(flush.len(), 1);
    assert_eq!(owners(flush[0]), vec!["Logger"]);
}
