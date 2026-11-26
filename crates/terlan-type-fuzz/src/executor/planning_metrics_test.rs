use super::*;

#[test]
fn compiler_work_is_aggregated_from_stable_timing_events() {
    let stderr = concat!(
        "ignored\n",
        "terlc timing: frontend.parse: CaseTest.terl: +14ms total=15ms\n",
        "terlc timing: frontend.parse: Result.terl: +6ms total=6ms\n",
        "terlc native-aot: application.generic-specialization: +3ms total=4ms\n",
        "terlc native-aot: generic-specialization.demo.value: 8ms specializations=3\n",
        "terlc native-aot: profiles-converged: elapsed=9ms phase=2 profiles=7 ",
        "lowered=11 cache-hits=5 widest=[]\n",
        "terlc native-aot: module-unit.begin.demo functions=13 continuations=2\n",
        "terlc native-aot: module-unit.begin.more functions=4 continuations=1\n",
        "terlc native-aot: image.cache-or-build: +4ms total=5ms\n",
        "terlc native-aot: publish.publish: +1ms total=4ms\n",
        "terlc native-aot: image.cache-or-build: +0ms total=1ms ",
        "reachable-declarations=17 native-providers=ndarray,polars ",
        "cache-miss-reason=source_changed\n",
        "terlc native-aot: image.types: semantic-types=2 aggregate-layouts=2 ",
        "collection-layouts=1 semantic-type-ids=0101,0202\n",
        "terlc native-aot: rooted.release-evidence: bindings=19 references=31\n",
        "terlc native-aot: rooted.cache: cache-hit=verified_checked_core_image\n",
    );
    let measured = measure(stderr);
    assert_eq!(measured.timing_events, 13);
    assert_eq!(measured.phase_elapsed_ms["frontend.parse"], 20);
    assert_eq!(
        measured.phase_elapsed_ms["application.generic-specialization"],
        3
    );
    assert_eq!(measured.generic_specializations, 3);
    assert_eq!(measured.call_profiles, 7);
    assert_eq!(measured.lowered_calls, 11);
    assert_eq!(measured.lowering_cache_hits, 5);
    assert_eq!(measured.native_images, 2);
    assert_eq!(measured.native_image_cache_hits, 2);
    assert_eq!(measured.native_image_cache_misses, 1);
    assert_eq!(measured.semantic_types, 2);
    assert_eq!(measured.aggregate_layouts, 2);
    assert_eq!(measured.collection_layouts, 1);
    assert_eq!(measured.semantic_inventory_events, 1);
    assert_eq!(measured.released_bindings, 19);
    assert_eq!(measured.released_references, 31);
    assert_eq!(
        measured.semantic_type_ids,
        ["0101".to_string(), "0202".to_string()].into()
    );
    assert_eq!(measured.native_units_built, 2);
    assert_eq!(measured.native_functions_built, 17);
    assert_eq!(measured.native_continuations_built, 3);
    assert_eq!(measured.last_phase.as_deref(), Some("rooted.cache"));
    assert_eq!(measured.reachable_declarations, Some(17));
    assert_eq!(
        measured.native_providers,
        ["ndarray".to_string(), "polars".to_string()].into()
    );
    assert_eq!(measured.cache_miss_reasons["source_changed"], 1);
}

#[test]
fn malformed_events_are_counted_without_inventing_work() {
    assert_eq!(phase_name("ignored", "terlc timing:"), None);
    let measured = measure(concat!(
        "terlc timing: incomplete\n",
        "terlc timing: wrong.unit: +7seconds\n",
        "terlc timing: wrong.value: +badms\n",
        "terlc native-aot: generic-specialization.bad specializations=no\n",
        "terlc native-aot: profiles-converged: profiles=no lowered=? cache-hits=none\n",
        "terlc native-aot: module-unit.begin.bad functions=x continuations=-\n",
    ));
    assert_eq!(measured.timing_events, 6);
    assert!(measured.phase_elapsed_ms.is_empty());
    assert_eq!(measured.generic_specializations, 0);
    assert_eq!(measured.call_profiles, 0);
    assert_eq!(measured.lowered_calls, 0);
    assert_eq!(measured.lowering_cache_hits, 0);
    assert_eq!(measured.native_images, 0);
    assert_eq!(measured.native_image_cache_hits, 0);
    assert_eq!(measured.native_image_cache_misses, 0);
    assert_eq!(measured.native_units_built, 1);
    assert_eq!(measured.native_functions_built, 0);
    assert_eq!(measured.native_continuations_built, 0);
    assert_eq!(measured.last_phase.as_deref(), Some("profiles-converged"));
    assert_eq!(measured.reachable_declarations, None);
    assert!(measured.native_providers.is_empty());
    assert!(measured.cache_miss_reasons.is_empty());
}

#[test]
fn comparison_reports_signed_work_deltas() {
    let left = CompilerMetrics {
        generic_specializations: 5,
        call_profiles: 4,
        lowered_calls: 3,
        lowering_cache_hits: 2,
        native_images: 1,
        native_image_cache_hits: 0,
        native_image_cache_misses: 1,
        semantic_types: 2,
        aggregate_layouts: 3,
        collection_layouts: 4,
        released_bindings: 10,
        released_references: 20,
        native_units_built: 1,
        native_functions_built: 9,
        native_continuations_built: 8,
        ..CompilerMetrics::default()
    };
    let right = CompilerMetrics {
        generic_specializations: 1,
        call_profiles: 2,
        lowered_calls: 3,
        lowering_cache_hits: 4,
        native_images: 5,
        native_image_cache_hits: 3,
        native_image_cache_misses: 2,
        semantic_types: 5,
        aggregate_layouts: 7,
        collection_layouts: 11,
        released_bindings: 14,
        released_references: 15,
        native_units_built: 5,
        native_functions_built: 6,
        native_continuations_built: 7,
        ..CompilerMetrics::default()
    };
    assert_eq!(
        delta(&right, &left),
        CompilerMetricDelta {
            generic_specializations: -4,
            call_profiles: -2,
            lowered_calls: 0,
            lowering_cache_hits: 2,
            native_images: 4,
            native_image_cache_hits: 3,
            native_image_cache_misses: 1,
            semantic_types: 3,
            aggregate_layouts: 4,
            collection_layouts: 7,
            released_bindings: 4,
            released_references: -5,
            native_units_built: 4,
            native_functions_built: -3,
            native_continuations_built: -1,
        }
    );
    assert!(!same_structural_work(&right, &left));
    assert!(same_structural_work(&right, &right));
}

#[test]
fn attribution_requires_every_planning_dimension() {
    let complete = measure(concat!(
        "terlc timing: frontend.parse: +1ms total=1ms\n",
        "terlc native-aot: planning.graph: reachable-declarations=1 native-providers=none\n",
        "terlc native-aot: rooted.cache: cache-hit=verified_checked_core_image\n",
        "terlc native-aot: image.types: semantic-types=0 aggregate-layouts=0 ",
        "collection-layouts=0 semantic-type-ids=none\n",
    ));
    assert!(attribution_gaps(&complete).is_empty());
    assert!(has_native_image_cache_hit(&complete));

    let missing = measure("");
    assert_eq!(
        attribution_gaps(&missing),
        [
            "phase timings",
            "reachable declarations",
            "cache disposition",
            "semantic type inventory",
        ]
    );
    assert!(!has_native_image_cache_hit(&missing));

    let inventory_without_identities = measure(
        "terlc native-aot: image.types: semantic-types=0 aggregate-layouts=0 collection-layouts=0\n",
    );
    assert_eq!(inventory_without_identities.semantic_inventory_events, 1);
    assert!(inventory_without_identities.semantic_type_ids.is_empty());
}

#[test]
fn combined_work_accepts_linear_growth_only() {
    let focused = CompilerMetrics {
        generic_specializations: 1,
        call_profiles: 1,
        lowered_calls: 1,
        native_units_built: 1,
        native_functions_built: 1,
        native_continuations_built: 1,
        semantic_types: 1,
        aggregate_layouts: 1,
        collection_layouts: 1,
        released_bindings: 1,
        released_references: 1,
        semantic_type_ids: ["one".to_string()].into(),
        reachable_declarations: Some(1),
        ..CompilerMetrics::default()
    };
    let linear = CompilerMetrics {
        generic_specializations: 4,
        call_profiles: 4,
        lowered_calls: 4,
        native_units_built: 4,
        native_functions_built: 4,
        native_continuations_built: 4,
        semantic_types: 4,
        aggregate_layouts: 4,
        collection_layouts: 4,
        released_bindings: 4,
        released_references: 4,
        semantic_type_ids: ["one".to_string(), "two".to_string()].into(),
        reachable_declarations: Some(4),
        ..CompilerMetrics::default()
    };
    assert!(combined_work_is_linear(&linear, &focused));

    for nonlinear in [
        CompilerMetrics {
            generic_specializations: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            call_profiles: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            lowered_calls: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            native_units_built: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            native_functions_built: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            native_continuations_built: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            semantic_types: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            aggregate_layouts: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            collection_layouts: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            released_bindings: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            released_references: 5,
            ..linear.clone()
        },
        CompilerMetrics {
            semantic_type_ids: (0..5).map(|value| value.to_string()).collect(),
            ..linear.clone()
        },
        CompilerMetrics {
            native_providers: ["unexpected".to_string()].into(),
            ..linear.clone()
        },
        CompilerMetrics {
            reachable_declarations: Some(5),
            ..linear.clone()
        },
        CompilerMetrics {
            reachable_declarations: None,
            ..linear.clone()
        },
    ] {
        assert!(!combined_work_is_linear(&nonlinear, &focused));
    }
    assert!(combined_work_is_linear(
        &CompilerMetrics::default(),
        &CompilerMetrics::default()
    ));
}
