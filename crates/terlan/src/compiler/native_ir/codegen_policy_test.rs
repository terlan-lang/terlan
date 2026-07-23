//! Development and release native code-generation policy tests.

use super::NativeCodegenPolicy;

/// Proves development compilation favors fast reusable module units.
#[test]
fn development_policy_is_fast_incremental_and_cache_distinct() {
    let policy = NativeCodegenPolicy::Development;

    assert_eq!(policy.cranelift_opt_level(), "none");
    assert!(policy.uses_incremental_module_units());
    assert!(!policy.optimizes_link());
    assert_eq!(
        policy.cache_identity(),
        "development-cranelift-none-modular-link-v1"
    );
}

/// Proves release compilation favors optimized whole-application emission.
#[test]
fn release_policy_is_optimized_whole_application_and_cache_distinct() {
    let policy = NativeCodegenPolicy::Release;

    assert_eq!(policy.cranelift_opt_level(), "speed");
    assert!(!policy.uses_incremental_module_units());
    assert!(policy.optimizes_link());
    assert_eq!(
        policy.cache_identity(),
        "release-cranelift-speed-whole-application-link-v1"
    );
    assert_ne!(
        policy.cache_identity(),
        NativeCodegenPolicy::Development.cache_identity()
    );
}

/// Proves live serving keeps optimized code without whole-image relinking.
#[test]
fn serve_policy_is_optimized_incremental_and_cache_distinct() {
    let policy = NativeCodegenPolicy::Serve;

    assert_eq!(policy.cranelift_opt_level(), "speed");
    assert!(policy.uses_incremental_module_units());
    assert!(!policy.optimizes_link());
    assert_eq!(
        policy.cache_identity(),
        "serve-cranelift-speed-modular-link-v1"
    );
    assert_ne!(
        policy.cache_identity(),
        NativeCodegenPolicy::Release.cache_identity()
    );
}
