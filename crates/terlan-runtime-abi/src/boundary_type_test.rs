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
