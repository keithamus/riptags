#![cfg(feature = "haskell")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const HASKELL_SOURCE: &str = r#"
module Store.Cache (Cache (..), newCache) where

import qualified Data.Map as Map
import Data.Maybe (fromMaybe)

data Cache = Cache
  { cacheName :: String
  , cacheSize :: Int
  }

data Status = Hit | Miss Int

newtype Key = Key String

type Entry = (Key, Int)

limit :: Int
limit = 128

class Sizable a where
  sizeOf :: a -> Int
  describe :: a -> String
  describe _ = "sizable"

instance Sizable Cache where
  sizeOf c = cacheSize c

newCache :: String -> Cache
newCache name = Cache { cacheName = name, cacheSize = 0 }

lookupKey :: Key -> Int
lookupKey (Key k) = fromMaybe 0 (Map.lookup k Map.empty)

classify :: Status -> String
classify Hit = "hit"
classify (Miss n) = helper n
  where
    helper x = show x

(<+>) :: Int -> Int -> Int
a <+> b = a + b

type family Elem c
data family Bucket c
type instance Elem Cache = Int

data Shape where
  Circle, Square :: Int -> Shape
"#;

#[test]
fn haskell_definitions_owners_and_references() {
    let tags = tag_source("Store/Cache.hs", HASKELL_SOURCE);

    // The module header names the file's module.
    let module = find(&tags, "Store.Cache", OccKind::Definition);
    assert_eq!(module.len(), 1);
    assert_eq!(module[0].sym_kind, SymKind::Module);

    // A top-level function: definition plus the separate type signature.
    let new_cache = find(&tags, "newCache", OccKind::Definition);
    assert_eq!(new_cache.len(), 1);
    assert_eq!(new_cache[0].sym_kind, SymKind::Function);
    assert_eq!(new_cache[0].line, 30);
    assert!(owners(new_cache[0]).is_empty());

    let new_cache_sig = find(&tags, "newCache", OccKind::Declaration);
    assert_eq!(new_cache_sig.len(), 1, "`newCache :: ...` is a declaration");
    assert_eq!(new_cache_sig[0].sym_kind, SymKind::Function);
    assert_eq!(new_cache_sig[0].line, 29);

    // A nullary top-level binding parses as `bind`, not `function`.
    let limit = find(&tags, "limit", OccKind::Definition);
    assert_eq!(limit.len(), 1);
    assert_eq!(limit[0].sym_kind, SymKind::Function);

    // `data` and `newtype` are classes; their constructors are value-level
    // constants owned by the type.
    let cache = find(&tags, "Cache", OccKind::Definition);
    assert_eq!(cache.len(), 2, "the type and its constructor");
    assert_eq!(cache[0].sym_kind, SymKind::Class);
    assert!(owners(cache[0]).is_empty());
    assert_eq!(cache[1].sym_kind, SymKind::Constant);
    assert_eq!(owners(cache[1]), vec!["Cache"]);

    let key = find(&tags, "Key", OccKind::Definition);
    assert_eq!(key.len(), 2, "newtype and its constructor");
    assert_eq!(key[0].sym_kind, SymKind::Class);
    assert_eq!(key[1].sym_kind, SymKind::Constant);
    assert_eq!(owners(key[1]), vec!["Key"]);

    let miss = find(&tags, "Miss", OccKind::Definition);
    assert_eq!(miss.len(), 1);
    assert_eq!(miss[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(miss[0]), vec!["Status"]);

    // Record fields are owned by the type that declares them.
    let field = find(&tags, "cacheName", OccKind::Definition);
    assert_eq!(field.len(), 1);
    assert_eq!(field[0].sym_kind, SymKind::Field);
    assert_eq!(owners(field[0]), vec!["Cache"]);

    // `type` synonyms.
    let entry = find(&tags, "Entry", OccKind::Definition);
    assert_eq!(entry.len(), 1);
    assert_eq!(entry[0].sym_kind, SymKind::Type);

    // A class is an interface; its members are owned by it.
    let sizable = find(&tags, "Sizable", OccKind::Definition);
    assert_eq!(sizable.len(), 1);
    assert_eq!(sizable[0].sym_kind, SymKind::Interface);

    let size_of_decl = find(&tags, "sizeOf", OccKind::Declaration);
    assert_eq!(size_of_decl.len(), 1);
    assert_eq!(size_of_decl[0].sym_kind, SymKind::Method);
    assert_eq!(owners(size_of_decl[0]), vec!["Sizable"]);
    assert_eq!(size_of_decl[0].context.as_deref(), Some("Sizable"));

    // A default method body in the class, and the instance implementation,
    // are both definitions owned by their enclosing scope.
    let describe_def = find(&tags, "describe", OccKind::Definition);
    assert_eq!(describe_def.len(), 1, "the class default implementation");
    assert_eq!(owners(describe_def[0]), vec!["Sizable"]);

    let size_of_def = find(&tags, "sizeOf", OccKind::Definition);
    assert_eq!(size_of_def.len(), 1, "the instance implementation");
    assert_eq!(size_of_def[0].sym_kind, SymKind::Method);
    assert_eq!(
        owners(size_of_def[0]),
        vec!["Sizable", "Cache"],
        "an instance method answers from the class and from the type"
    );

    // The instance head references both the class and the instantiated type.
    let impls = find(&tags, "Sizable", OccKind::Impl);
    assert_eq!(impls.len(), 1);
    assert_eq!(impls[0].line, 26);
    let impl_type = find(&tags, "Cache", OccKind::Impl);
    assert_eq!(impl_type.len(), 1);
    assert_eq!(impl_type[0].line, 26);

    // Imports: the module path, its alias, and the explicitly imported name.
    let import_map = find(&tags, "Data.Map", OccKind::Import);
    assert_eq!(import_map.len(), 1);
    let alias = find(&tags, "Map", OccKind::Import);
    assert_eq!(alias.len(), 1, "`as Map` is the name used at call sites");
    let from_maybe_import = find(&tags, "fromMaybe", OccKind::Import);
    assert_eq!(from_maybe_import.len(), 1);
    assert_eq!(owners(from_maybe_import[0]), vec!["Maybe"]);

    // Calls: bare application heads, and qualified ones resolving their owner
    // through `@qualifier`.
    let qualified_call = find(&tags, "lookup", OccKind::Call);
    assert_eq!(qualified_call.len(), 1);
    assert_eq!(
        owners(qualified_call[0]),
        vec!["Map"],
        "`Map.lookup` is owned by the qualifier"
    );
    assert_eq!(qualified_call[0].context.as_deref(), Some("lookupKey"));

    let empty = find(&tags, "empty", OccKind::Path);
    assert_eq!(empty.len(), 1, "`Map.empty` is a qualified mention");
    assert_eq!(owners(empty[0]), vec!["Map"]);

    let from_maybe = find(&tags, "fromMaybe", OccKind::Call);
    assert_eq!(from_maybe.len(), 1);
    assert!(owners(from_maybe[0]).is_empty());

    let size_call = find(&tags, "cacheSize", OccKind::Call);
    assert_eq!(size_call.len(), 1);
    assert_eq!(size_call[0].context.as_deref(), Some("Sizable::sizeOf"));

    // Constructor mentions in expressions and patterns.
    let hit = find(&tags, "Hit", OccKind::Path);
    assert_eq!(hit.len(), 1, "the pattern match on `Hit`");

    // Record construction names the fields it sets.
    let field_use = find(&tags, "cacheSize", OccKind::Field);
    assert_eq!(field_use.len(), 1);

    // Type mentions.
    let cache_types = find(&tags, "Cache", OccKind::Type);
    assert_eq!(
        cache_types.len(),
        3,
        "the export list, the `newCache` signature and the type instance: {cache_types:?}"
    );

    // A user-defined operator is a searchable name at both ends.
    let op_decl = find(&tags, "<+>", OccKind::Declaration);
    assert_eq!(op_decl.len(), 1);
    assert_eq!(op_decl[0].sym_kind, SymKind::Function);
    let op_def = find(&tags, "<+>", OccKind::Definition);
    assert_eq!(op_def.len(), 1);
    assert_eq!(op_def[0].sym_kind, SymKind::Function);
    assert_eq!(find(&tags, "+", OccKind::Call).len(), 1, "an operator use");

    // Locals stay out of the index: a `where` binding is not a definition.
    assert!(
        find(&tags, "helper", OccKind::Definition).is_empty(),
        "a `where` binding is a local, not a searchable definition"
    );
    assert!(
        find(&tags, "c", OccKind::Definition).is_empty(),
        "parameters are never tagged"
    );

    // Families, their instances and a grouped GADT constructor line.
    let elem = find(&tags, "Elem", OccKind::Definition);
    assert_eq!(elem.len(), 2, "the family head and its instance");
    assert!(elem.iter().all(|tag| tag.sym_kind == SymKind::Type));
    let bucket = find(&tags, "Bucket", OccKind::Definition);
    assert_eq!(bucket.len(), 1);
    assert_eq!(bucket[0].sym_kind, SymKind::Class);
    for name in ["Circle", "Square"] {
        let ctor = find(&tags, name, OccKind::Definition);
        assert_eq!(ctor.len(), 1, "`Circle, Square :: ...` defines both");
        assert_eq!(ctor[0].sym_kind, SymKind::Constant);
        assert_eq!(owners(ctor[0]), vec!["Shape"]);
    }
}
