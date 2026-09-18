use std::collections::HashSet;

use riptags::tagfile::{TagFile, TagFileWriter, TagView};
use riptags::{OccKind, SymKind, Tag};

fn tag(name: &str, line: u32, kind: OccKind, owners: &[&str]) -> Tag {
    Tag {
        line,
        col: 0,
        len: name.len() as u32,
        kind,
        sym_kind: SymKind::Function,
        name: Box::from(name),
        owners: owners.iter().map(|owner| Box::from(*owner)).collect(),
        context: None,
    }
}

/// Every `owner#name` the tags mention gets exactly one record, and nothing
/// else does: a symbol split across two records would hide half its callsites
/// from the binary search that finds it.
#[test]
fn the_symbol_table_holds_each_symbol_once() {
    let tags = vec![
        tag("alpha", 1, OccKind::Definition, &["Thing"]),
        tag("alpha", 2, OccKind::Call, &[]),
        // An owner with no text is no owner: it must fold into the bare form
        // rather than opening a second record for the same symbol.
        tag("beta", 3, OccKind::Definition, &[""]),
        // The same owner twice is one symbol, and one occurrence of it.
        tag("gamma", 4, OccKind::Definition, &["Dup", "Dup"]),
        tag("gamma", 5, OccKind::Call, &["Other"]),
    ];
    let mut writer = TagFileWriter::default();
    writer.file("src/lib.rs", 1, 2);
    for tag in &tags {
        writer.push(TagView::of(tag));
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("index.tags");
    std::fs::write(&path, writer.finish()).unwrap();
    let map = TagFile::open(&path).expect("the index maps");

    let held: Vec<(String, String, usize, usize)> = map
        .symbols()
        .map(|sym| {
            (
                sym.owner().to_string(),
                sym.name().to_string(),
                sym.occurrences(),
                sym.definitions(),
            )
        })
        .collect();
    let expected = [
        ("".to_string(), "alpha".to_string(), 2, 1),
        ("Thing".to_string(), "alpha".to_string(), 1, 1),
        ("".to_string(), "beta".to_string(), 1, 1),
        ("".to_string(), "gamma".to_string(), 2, 1),
        ("Dup".to_string(), "gamma".to_string(), 1, 1),
        ("Other".to_string(), "gamma".to_string(), 1, 0),
    ];
    let unique: HashSet<(&String, &String)> = held.iter().map(|(o, n, _, _)| (o, n)).collect();
    assert_eq!(unique.len(), held.len(), "no symbol has two records");
    assert_eq!(
        held.iter().collect::<HashSet<_>>(),
        expected.iter().collect::<HashSet<_>>()
    );

    let names: Vec<(&str, &str)> = map.symbols().map(|sym| (sym.name(), sym.owner())).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "the table is ordered for binary search");

    let alpha = map.symbol("Thing", "alpha").expect("owner-qualified");
    assert_eq!(
        alpha
            .slots()
            .map(|(slot, kind)| (map.tag(slot).unwrap().line, kind))
            .collect::<Vec<_>>(),
        vec![(1, OccKind::Definition)]
    );
    assert_eq!(map.symbol("", "gamma").unwrap().occurrences(), 2);
    assert!(map.symbol("Missing", "alpha").is_none());
}
