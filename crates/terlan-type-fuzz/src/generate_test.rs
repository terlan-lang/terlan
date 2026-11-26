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
    let cases = (0..17).map(|index| case(1, index, 8)).collect::<Vec<_>>();
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
            "closed_union_empty"
        ]
    );
    for case in cases {
        assert_eq!(case.manifest.expected_type, case.expected.type_name);
        assert!(!case.source.contains(" as "));
        assert!(case
            .source
            .contains("@test\npub sample_matches_expected(): Bool"));
        assert!(case.source.contains("sample() =="));
    }
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
    for index in 0..10_000 {
        let generated = case(17, index, 8);
        assert_eq!(
            generated.manifest.expected_type,
            generated.expected.type_name
        );
        assert!(!generated.source.contains(" as "));
        assert!(identities.insert(generated.manifest.case_id));
        families.insert(generated.manifest.family);
    }
    assert_eq!(families.len(), 17);
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
}
