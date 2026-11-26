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
    );
    let measured = measure(stderr);
    assert_eq!(measured.timing_events, 7);
    assert_eq!(measured.phase_elapsed_ms["frontend.parse"], 20);
    assert_eq!(
        measured.phase_elapsed_ms["application.generic-specialization"],
        3
    );
    assert_eq!(measured.generic_specializations, 3);
    assert_eq!(measured.call_profiles, 7);
    assert_eq!(measured.lowered_calls, 11);
    assert_eq!(measured.lowering_cache_hits, 5);
    assert_eq!(measured.native_units_built, 2);
    assert_eq!(measured.native_functions_built, 17);
    assert_eq!(measured.native_continuations_built, 3);
}

#[test]
fn malformed_events_are_counted_without_inventing_work() {
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
    assert_eq!(measured.native_units_built, 1);
    assert_eq!(measured.native_functions_built, 0);
    assert_eq!(measured.native_continuations_built, 0);
}

#[test]
fn comparison_reports_signed_work_deltas() {
    let left = CompilerMetrics {
        generic_specializations: 5,
        call_profiles: 4,
        lowered_calls: 3,
        lowering_cache_hits: 2,
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
            native_units_built: 4,
            native_functions_built: -3,
            native_continuations_built: -1,
        }
    );
}
