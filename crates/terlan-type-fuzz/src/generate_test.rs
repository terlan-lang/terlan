use super::*;

fn case(seed: u64, index: u32, depth: u8) -> GeneratedCase {
    generate(
        index,
        &GenerationConfig {
            seed,
            max_depth: depth,
            oracle: Oracle::None,
            timeout_ms: 2_000,
            rss_limit_bytes: 1024,
            cpu_limit_ms: 2_000,
            compiler_revision: "test",
        },
    )
}

#[test]
fn generation_is_stable_and_covers_each_initial_family() {
    assert_eq!(case(17, 3, 8), case(17, 3, 8));
    let cases = (0..19).map(|index| case(1, index, 8)).collect::<Vec<_>>();
    let families = cases
        .iter()
        .map(|case| case.manifest.family.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        families,
        [
            "scalar_unit",
            "scalar_bool",
            "scalar_int",
            "scalar_float",
            "scalar_string",
            "scalar_atom",
            "lexical_product",
            "record",
            "list",
            "if_product_join",
            "case_product_join",
            "option_join",
            "nominal_option_join",
            "result_join",
            "nested_result_join",
            "closed_union",
            "closed_union_empty",
            "generic_field_receiver",
            "generic_trait_package"
        ]
    );
    for case in cases {
        assert_eq!(case.manifest.expected_type, case.expected.type_name);
        assert!(!case.source.contains(" as Dynamic"));
        assert!(case
            .source
            .contains("@test\npub sample_matches_expected(): Bool"));
        assert!(case.source.contains("sample() =="));
    }
    let module_case = &case(1, 17, 8);
    assert_eq!(module_case.support_sources.len(), 4);
    assert!(module_case.project.contains("source_roots = [\"src\"]"));
    assert!(!module_case.project.contains("source_roots = [\".\"]"));
    assert!(module_case
        .support_sources
        .iter()
        .all(|source| source.path.starts_with("src")));
    assert!(module_case.source.contains("batch.latest_sequence()"));
    assert!(module_case.source.contains("Numbers.next("));
    assert!(module_case.support_sources.iter().any(|source| source
        .source
        .contains("batch.state.execution_state().latest_sequence()")));
    let trait_case = &case(1, 18, 8);
    assert_eq!(trait_case.support_sources.len(), 3);
    assert!(trait_case.source.contains("Score.score(value)"));
    assert!(trait_case.source.contains("Ready as LeftReady"));
    assert!(trait_case.source.contains("RightStopped()"));
    assert_eq!(trait_case.expected.value, serde_json::json!(255));
    assert!(trait_case
        .support_sources
        .iter()
        .any(|source| source.source.contains("Value implements Score[Value]")));
    assert_eq!(
        trait_case
            .support_sources
            .iter()
            .filter(|source| source.source.contains("pub struct Value"))
            .count(),
        2
    );
}

#[test]
fn seed_and_depth_budgets_change_only_their_owned_dimensions() {
    assert_ne!(case(1, 2, 8).expected, case(2, 2, 8).expected);
    for index in 0..12 {
        assert!(case(1, index, 1).manifest.family.starts_with("scalar_"));
    }
    assert_eq!(case(1, 10, 2).manifest.family, "scalar_unit");
}

#[test]
fn ten_thousand_cases_preserve_type_and_identity_invariants() {
    let mut identities = std::collections::HashSet::new();
    let mut families = std::collections::HashSet::new();
    let mut support_paths = std::collections::HashSet::new();
    let mut support_count = 0;
    for index in 0..10_000 {
        let generated = case(17, index, 8);
        assert_eq!(
            generated.manifest.expected_type,
            generated.expected.type_name
        );
        assert!(!generated.source.contains(" as Dynamic"));
        for support in &generated.support_sources {
            assert!(!support.source.contains(" as "));
            assert!(support_paths.insert(support.path.clone()));
            support_count += 1;
        }
        assert!(identities.insert(generated.manifest.case_id));
        families.insert(generated.manifest.family);
    }
    assert_eq!(families.len(), 19);
    assert_eq!(support_paths.len(), support_count);
    assert_eq!(support_count, 3_682);
}

#[test]
fn checked_oracle_is_encoded_in_identity_and_manifest() {
    let generated = generate(
        0,
        &GenerationConfig {
            seed: 1,
            max_depth: 8,
            oracle: Oracle::Check,
            timeout_ms: 2_000,
            rss_limit_bytes: 1024,
            cpu_limit_ms: 2_000,
            compiler_revision: "test",
        },
    );
    assert_eq!(generated.manifest.oracle, Oracle::Check);
    assert!(generated.manifest.case_id.ends_with("-ocheck"));

    let native = generate(
        0,
        &GenerationConfig {
            seed: 1,
            max_depth: 8,
            oracle: Oracle::Native,
            timeout_ms: 2_000,
            rss_limit_bytes: 1024,
            cpu_limit_ms: 2_000,
            compiler_revision: "test",
        },
    );
    assert_eq!(native.manifest.oracle, Oracle::Native);
    assert!(native.manifest.case_id.ends_with("-onative"));
    assert!(native.manifest.case_id.starts_with("v3-"));
    assert_eq!(native.manifest.generator, GENERATOR_V3);
}

#[test]
fn recorded_generator_versions_retain_their_family_mapping() {
    let config = GenerationConfig {
        seed: 1,
        max_depth: 8,
        oracle: Oracle::Check,
        timeout_ms: 2_000,
        rss_limit_bytes: 1024,
        cpu_limit_ms: 2_000,
        compiler_revision: "test",
    };
    let legacy = regenerate(17, &config, GENERATOR_V1).expect("legacy generator");
    assert_eq!(legacy.manifest.family, "scalar_unit");
    assert!(legacy.manifest.case_id.starts_with("v1-"));
    assert!(legacy.support_sources.is_empty());

    let second = regenerate(18, &config, GENERATOR_V2).expect("second generator");
    assert_eq!(second.manifest.family, "scalar_unit");
    assert!(second.manifest.case_id.starts_with("v2-"));

    let current = regenerate(18, &config, GENERATOR_V3).expect("current generator");
    assert_eq!(current.manifest.family, "generic_trait_package");
    assert!(current.manifest.case_id.starts_with("v3-"));
    assert_eq!(current.support_sources.len(), 3);
    assert_eq!(
        regenerate(18, &config, "terlan.type-fuzz-generator.v4").unwrap_err(),
        "unsupported type fuzz generator `terlan.type-fuzz-generator.v4`"
    );
}

#[test]
fn javascript_oracle_uses_only_the_explicit_supported_matrix() {
    let generated = (0..14)
        .map(|index| {
            generate(
                index,
                &GenerationConfig {
                    seed: 1,
                    max_depth: 8,
                    oracle: Oracle::JavaScript,
                    timeout_ms: 2_000,
                    rss_limit_bytes: 1024,
                    cpu_limit_ms: 2_000,
                    compiler_revision: "test",
                },
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        generated
            .iter()
            .map(|case| case.manifest.family.as_str())
            .collect::<Vec<_>>(),
        [
            "scalar_unit",
            "scalar_bool",
            "scalar_int",
            "scalar_float",
            "scalar_string",
            "scalar_atom",
            "lexical_product",
            "record",
            "list",
            "if_product_join",
            "case_product_join",
            "option_join",
            "result_join",
            "nested_result_join",
        ]
    );
    assert!(generated
        .iter()
        .all(|case| case.manifest.oracle == Oracle::JavaScript
            && case.manifest.case_id.ends_with("-ojs")));
    assert_eq!(
        generate(
            13,
            &GenerationConfig {
                seed: 1,
                max_depth: 2,
                oracle: Oracle::JavaScript,
                timeout_ms: 2_000,
                rss_limit_bytes: 1024,
                cpu_limit_ms: 2_000,
                compiler_revision: "test",
            },
        )
        .manifest
        .family,
        "scalar_float"
    );
}
