use super::*;

/// Packed names retain canonical byte ordering, Unicode boundaries and exact indexes.
#[test]
fn atom_names_share_one_contiguous_utf8_buffer() {
    let names = ["東京", "λ", "ready", "éclair", "λ", "error"];
    let table = AtomTable::new(names).unwrap();
    let mut expected = names.to_vec();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(table.identities().collect::<Vec<_>>(), expected);
    assert_eq!(
        table.text.len(),
        expected.iter().map(|name| name.len()).sum::<usize>()
    );
    let mut prior = 0;
    for (index, expected) in expected.into_iter().enumerate() {
        let range = &table.ranges[index];
        assert_eq!(range.start, prior);
        assert!(table.text.is_char_boundary(range.end as usize));
        let atom = table.index(expected).unwrap();
        assert_eq!(atom.get(), index as u32);
        let actual = table.identity(atom).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            actual.as_ptr(),
            table.text.as_ptr().wrapping_add(range.start as usize)
        );
        prior = range.end;
    }
    assert_eq!(prior as usize, table.text.len());
    assert_eq!(table.clone(), table);
}

/// Offset overflow is rejected without requiring a multi-gigabyte fixture allocation.
#[test]
fn packed_atom_offsets_reject_overflow() {
    assert_eq!(packed_span(0, 2), Ok(0..2));
    assert_eq!(
        packed_span(u32::MAX as usize, 1),
        Err(ManagedMemoryError::TooManyAtoms)
    );
    assert_eq!(
        packed_span(usize::MAX, 1),
        Err(ManagedMemoryError::TooManyAtoms)
    );
    assert_eq!(
        AtomTable::default().identity(AtomIndex::from_runtime(0)),
        Err(ManagedMemoryError::UnknownAtom)
    );
}

/// Every admitted index has the same meaning as the original individually owned names.
#[test]
fn packed_atom_lookup_matches_owned_names_across_table_sizes() {
    for count in [0, 1, 8, 128, 1025] {
        let mut names = (0..count)
            .rev()
            .map(|index| format!("name_{index:08}_λ"))
            .collect::<Vec<_>>();
        if let Some(first) = names.first().cloned() {
            names.push(first);
        }
        let table = AtomTable::new(names.clone()).unwrap();
        names.sort_unstable();
        names.dedup();
        assert_eq!(table.len(), names.len());
        for (index, name) in names.iter().enumerate() {
            let atom = table.index(name).unwrap();
            assert_eq!(atom.get(), index as u32);
            assert_eq!(table.identity(atom), Ok(name.as_str()));
        }
        assert_eq!(table.index("unknown"), Err(ManagedMemoryError::UnknownAtom));
        assert_eq!(
            table.identity(AtomIndex::from_runtime(count)),
            Err(ManagedMemoryError::UnknownAtom)
        );
    }
}
