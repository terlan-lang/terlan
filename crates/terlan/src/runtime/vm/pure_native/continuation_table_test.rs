use super::*;
use crate::runtime::native_image::TvmBoundaryType;

/// Sparse stable IDs resolve their admitted signatures regardless of input order.
#[test]
fn continuation_lookup_indexes_unordered_and_sparse_admitted_ids() {
    let table = NativeContinuationTable::from(
        [u64::MAX, 9, 0, 451]
            .map(|id| TvmContinuationDescriptor {
                id,
                parameters: vec![TvmBoundaryType::Int],
                results: vec![TvmBoundaryType::Bool],
            })
            .to_vec(),
    );
    for id in [0, 9, 451, u64::MAX] {
        let entry = table.get(id).expect("admitted continuation");
        assert_eq!(entry.id, id);
        assert_eq!(entry.parameters, [TvmBoundaryType::Int]);
        assert_eq!(entry.results, [TvmBoundaryType::Bool]);
    }
    for missing in [1, 450, u64::MAX - 1] {
        assert!(table.get(missing).is_none());
    }
    assert_eq!(table.ids().collect::<Vec<_>>(), [0, 9, 451, u64::MAX]);
    assert!(NativeContinuationTable::from(Vec::new()).get(0).is_none());
}

/// Clones retain one immutable allocation through thread transfer and owner exit.
#[test]
fn continuation_metadata_is_shared_and_outlives_the_original_owner() {
    let table = NativeContinuationTable::from(
        (0..8192)
            .rev()
            .map(|id| TvmContinuationDescriptor {
                id,
                parameters: vec![TvmBoundaryType::Int; 8],
                results: vec![TvmBoundaryType::Bool],
            })
            .collect::<Vec<_>>(),
    );
    let moved = table.clone();
    assert!(Arc::ptr_eq(&table.0, &moved.0));
    assert!(std::ptr::eq(
        table.get(4096).unwrap(),
        moved.get(4096).unwrap()
    ));
    let original = Arc::downgrade(&table.0);
    drop(table);
    let moved = std::thread::spawn(move || {
        assert_eq!(moved.get(8191).unwrap().parameters.len(), 8);
        moved
    })
    .join()
    .expect("immutable metadata can cross scheduler threads");
    assert_eq!(original.strong_count(), 1);
    drop(moved);
    assert!(original.upgrade().is_none());
}
