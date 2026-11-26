use super::*;

#[test]
fn rooted_cache_events_attribute_every_outcome() {
    assert!(rooted_cache_event(RootedCacheDisposition::Disabled)
        .contains("cache-miss-reason=incremental_reuse_disabled"));
    assert!(rooted_cache_event(RootedCacheDisposition::Invalidated)
        .contains("cache-miss-reason=checked_core_or_artifact_changed"));
    assert!(rooted_cache_event(RootedCacheDisposition::Hit)
        .contains("cache-hit=verified_checked_core_image"));
    trace_rooted_cache(false, RootedCacheDisposition::Hit);
    trace_rooted_cache(true, RootedCacheDisposition::Hit);
}

#[test]
fn semantic_inventory_is_sorted_deduplicated_and_explicitly_empty() {
    assert!(native_semantic_inventory_event(&[], &[]).contains(
        "semantic-types=0 aggregate-layouts=0 collection-layouts=0 semantic-type-ids=none"
    ));

    let aggregates = [[2; 16], [1; 16]];
    let collections = [[2; 16]];
    assert_eq!(
        native_semantic_inventory_event(&aggregates, &collections),
        concat!(
            "terlc native-aot: image.types: semantic-types=2 aggregate-layouts=2 ",
            "collection-layouts=1 semantic-type-ids=01010101010101010101010101010101,",
            "02020202020202020202020202020202",
        )
    );
}

#[test]
fn rooted_native_input_releases_frontend_binding_evidence() {
    let state = crate::CliState::default();
    let source = concat!(
        "module native_evidence_release.\n",
        "pub main(value: Int): Int ->\n",
        "    let next = value + 1;\n",
        "    next.\n",
    );
    let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "native_evidence_release.terl",
        source,
        state.diagnostic_format,
        None,
        state.native_policy,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("check binding-evidence fixture");
    let mut cores = vec![compiled.core];
    assert!(!cores[0].binding_identities.bindings.is_empty());
    assert!(!cores[0].binding_identities.references.is_empty());

    release_frontend_binding_evidence(&mut cores, false);

    assert!(cores[0].binding_identities.bindings.is_empty());
    assert!(cores[0].binding_identities.references.is_empty());
    cores[0]
        .binding_identities
        .validate()
        .expect("released evidence remains a valid empty inventory");
    release_frontend_binding_evidence(&mut cores, true);
}
