//! Atom rendering keeps image-local identities and actor-owned allocation.

use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits};

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(118).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

#[test]
fn atom_text_uses_the_current_image_table_and_managed_string_result() {
    let encoded = encode_atom_to_string_operation();
    assert!(super::super::managed_abi_result_is_reference(&encoded));
    let mut heap = heap();
    for atoms in [vec!["ready", "éclair"], vec!["aardvark", "ready", "éclair"]] {
        let atoms = atoms.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let layouts = ManagedLayoutRegistry::from_image(&[], &[], &atoms).expect("atom table");
        for name in atoms {
            let index = layouts.atom_index(&name).expect("known atom");
            let result = super::super::execute_managed_operation_with_context(
                &mut heap,
                &layouts,
                None,
                &encoded,
                &[i64::from(index.get())],
            )
            .expect("render atom");
            let reference = reference_word(i64::try_from(result).expect("reference word"))
                .expect("managed reference");
            assert_eq!(heap.read_string(reference.cast()), Ok(name.as_str()));
        }
    }
}

#[test]
fn atom_text_rejects_invalid_indexes_and_malformed_operations() {
    let layouts =
        ManagedLayoutRegistry::from_image(&[], &[], &["ready".to_owned()]).expect("atom table");
    let mut heap = heap();
    let encoded = encode_atom_to_string_operation();
    for index in [-1, 1, i64::from(u32::MAX) + 1] {
        assert_eq!(
            execute_string_operation(&mut heap, &layouts, &encoded, &[index]),
            Err(ManagedMemoryError::UnknownAtom)
        );
    }
    for words in [vec![], vec![0, 0]] {
        assert_eq!(
            execute_string_operation(&mut heap, &layouts, &encoded, &words),
            Err(ManagedMemoryError::InvalidManagedOperation)
        );
    }
    let mut malformed = vec![encoded[..7].to_vec()];
    let mut trailing = encoded.clone();
    trailing.push(0);
    malformed.push(trailing);
    for (position, value) in [(4, 2), (6, u8::MAX), (7, 1)] {
        let mut changed = encoded.clone();
        changed[position] = value;
        malformed.push(changed);
    }
    for encoded in malformed {
        assert_eq!(
            execute_string_operation(&mut heap, &layouts, &encoded, &[0]),
            Err(ManagedMemoryError::InvalidManagedOperation)
        );
    }
}
