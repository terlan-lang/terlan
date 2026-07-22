use super::*;

#[test]
fn atom_table_is_sorted_finite_and_generation_local() {
    let table = AtomTable::new(["ready", "error", "ready", "ok"]).expect("atom table");
    let identities = table.identities().collect::<Vec<_>>();

    assert_eq!(identities, vec!["error", "ok", "ready"]);
    assert_eq!(table.len(), 3);
    assert!(!table.is_empty());
    assert_eq!(table.index("ok").expect("known atom").get(), 1);
    assert_eq!(
        table.identity(table.index("ready").expect("index")),
        Ok("ready")
    );
    assert_eq!(table.index("ok").expect("index").to_string(), "1");

    let reordered = AtomTable::new(["ok", "ready", "error"]).expect("reordered table");
    assert_eq!(table, reordered);
}

#[test]
fn atom_table_rejects_dynamic_or_malformed_identities() {
    let empty = AtomTable::new(Vec::<String>::new()).expect("empty atom table");
    assert!(empty.is_empty());
    assert_eq!(
        empty.identity(AtomIndex(0)),
        Err(ManagedMemoryError::UnknownAtom)
    );
    assert_eq!(empty.index("missing"), Err(ManagedMemoryError::UnknownAtom));
    assert_eq!(
        AtomTable::new([""]),
        Err(ManagedMemoryError::EmptyAtomIdentity)
    );
    assert_eq!(
        AtomTable::new(["bad\0atom"]),
        Err(ManagedMemoryError::InvalidAtomIdentity)
    );
    assert_eq!(
        AtomTable::new(["bad\natom"]),
        Err(ManagedMemoryError::InvalidAtomIdentity)
    );
    assert_eq!(
        AtomTable::new(["known"]).expect("table").index(""),
        Err(ManagedMemoryError::EmptyAtomIdentity)
    );
}
