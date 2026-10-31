use super::{validate_transition_arguments, TvmTransitionOperation};

#[test]
fn lifecycle_teardown_transitions_require_one_positive_identity() {
    for operation in [
        TvmTransitionOperation::Unlink,
        TvmTransitionOperation::Demonitor,
        TvmTransitionOperation::ResourceRelease,
    ] {
        assert!(validate_transition_arguments(&operation, &[1]).is_ok());
        assert!(validate_transition_arguments(&operation, &[]).is_err());
        assert!(validate_transition_arguments(&operation, &[0]).is_err());
        assert!(validate_transition_arguments(&operation, &[1, 2]).is_err());
    }
}
