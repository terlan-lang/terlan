use super::is_compiler_generated_callable;

#[test]
fn declaration_coverage_excludes_generated_aot_callables() {
    assert!(is_compiler_generated_callable(
        "$aot_closure_factory_without_cache_default_0"
    ));
    assert!(is_compiler_generated_callable(
        "$aot_hof_context__aot_comprehension_without_cache_default_1_0_0_3_0"
    ));
    assert!(!is_compiler_generated_callable(
        "$aot_generic_std.sync.Resource.inserted_0"
    ));
    assert!(!is_compiler_generated_callable("without_cache_default"));
    assert!(!is_compiler_generated_callable("$closure_user_named"));
}
