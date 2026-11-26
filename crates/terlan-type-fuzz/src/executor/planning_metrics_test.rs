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
        "terlc native-aot: rooted.cache: cache-hit=verified_checked_core_image\n",
    );
    let measured = measure(stderr);
    assert_eq!(measured.timing_events, 11);
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
            native_units_built: 4,
            native_functions_built: -3,
            native_continuations_built: -1,
        }
    );
    assert!(!same_structural_work(&right, &left));
    assert!(same_structural_work(&right, &right));
}
