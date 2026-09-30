use super::TvmBoundaryType;

#[test]
fn transition_words_roundtrip_all_boundary_kinds() {
    for boundary_type in [
        TvmBoundaryType::Unit,
        TvmBoundaryType::Bool,
        TvmBoundaryType::Int,
        TvmBoundaryType::Float,
        TvmBoundaryType::Binary,
        TvmBoundaryType::String,
        TvmBoundaryType::Json,
        TvmBoundaryType::NativeResource(42),
        TvmBoundaryType::Atom,
        TvmBoundaryType::Bytes,
        TvmBoundaryType::Managed([7; 16]),
    ] {
        assert_eq!(
            TvmBoundaryType::from_transition_words(&boundary_type.transition_words())
                .expect("valid transition words"),
            boundary_type
        );
    }
}

#[test]
fn malformed_headers_fail_closed_and_resource_ids_preserve_all_bits() {
    for words in [
        vec![],
        vec![0],
        vec![0, 0],
        vec![0, 0, 0, 0],
        vec![7, 0, 1],
        vec![-1, 0, 0],
        vec![11, 0, 0],
        vec![i64::MAX, 0, 0],
    ] {
        let error = TvmBoundaryType::from_transition_words(&words).unwrap_err();
        assert_eq!(error.code(), "tvm.transition.boundary_type");
    }
    for tag in [0, 1, 2, 3, 4, 5, 6, 8, 9] {
        for (low, high) in [(1, 0), (0, 1), (-1, -1)] {
            assert!(TvmBoundaryType::from_transition_words(&[tag, low, high]).is_err());
        }
    }
    for id in [0, i64::MAX as u64, 1_u64 << 63, u64::MAX] {
        let ty = TvmBoundaryType::NativeResource(id);
        assert_eq!(
            TvmBoundaryType::from_transition_words(&ty.transition_words()).unwrap(),
            ty
        );
    }
    let ty = TvmBoundaryType::Managed([255; 16]);
    assert_eq!(
        TvmBoundaryType::from_transition_words(&ty.transition_words()).unwrap(),
        ty
    );
}

#[test]
fn only_heap_backed_boundaries_are_managed_references() {
    for ty in [
        TvmBoundaryType::Binary,
        TvmBoundaryType::String,
        TvmBoundaryType::Bytes,
        TvmBoundaryType::Managed([0; 16]),
    ] {
        assert!(ty.is_managed_reference());
    }
    for ty in [
        TvmBoundaryType::Unit,
        TvmBoundaryType::Bool,
        TvmBoundaryType::Int,
        TvmBoundaryType::Float,
        TvmBoundaryType::Json,
        TvmBoundaryType::NativeResource(1),
        TvmBoundaryType::Atom,
    ] {
        assert!(!ty.is_managed_reference());
    }
}
