use super::*;

/// Verifies synthetic VM sources scale helper count and expected result.
///
/// Inputs:
/// - Module name and helper count.
///
/// Output:
/// - Test passes when generated source contains the requested helpers and
///   stable assertion expression.
///
/// Transformation:
/// - Protects the project/large-app-sized benchmark source generator from
///   silently shrinking to a tiny single-expression workload.
#[test]
pub(super) fn synthetic_helper_source_contains_requested_workload() {
    let source = synthetic_helper_source("bench.Sample", 5);

    assert!(source.contains("module bench.Sample."));
    assert!(source.contains("helper0(value: Int): Int"));
    assert!(source.contains("helper4(value: Int): Int"));
    assert!(source.contains("block0(): Int"));
    assert!(source.contains("helper4(helper3(helper2(helper1(helper0(1)))))"));
    assert!(source.contains("pub main(): Bool ->\n    block0() == 11."));
}

/// Verifies the VM performance baseline records all deliberate pending lanes.
///
/// Inputs:
/// - Static skipped-track list.
///
/// Output:
/// - Test passes when the skipped tracks match the exact required list.
///
/// Transformation:
/// - Keeps skipped VM tracks explicit. The current required list is empty
///   because previously pending lanes have been promoted to measurements.
#[test]
pub(super) fn vm_performance_skipped_tracks_match_required_policy() {
    let tracks = vm_performance_skipped_tracks();
    let names = tracks.iter().map(|track| track.name).collect::<Vec<_>>();
    let expected_names = REQUIRED_VM_SKIPPED_TRACKS
        .iter()
        .map(|track| track.name)
        .collect::<Vec<_>>();

    assert_eq!(names, expected_names);
    assert_eq!(names.len(), REQUIRED_VM_SKIPPED_TRACKS.len());
    for (index, name) in names.iter().enumerate() {
        assert!(
            !names[..index].contains(name),
            "duplicate skipped performance track `{name}`"
        );
    }
    for track in &tracks {
        assert!(
            track.reason.starts_with("vm_") || track.reason.starts_with("terlan_vm_"),
            "skipped track `{}` has untyped reason `{}`",
            track.name,
            track.reason
        );
        assert!(
            !track.detail.trim().is_empty(),
            "skipped track `{}` has empty detail",
            track.name
        );
    }
}

/// Verifies direct VM primitive benchmark tracks execute correctness
/// assertions independently from source-level VM execution.
///
/// Inputs:
/// - VM table, actor, and timer primitive benchmark operations.
///
/// Output:
/// - Test passes when all primitive operations complete successfully.
///
/// Transformation:
/// - Prevents the performance baseline from reporting primitive timings
///   before the underlying runtime operations are actually exercised.
#[test]
pub(super) fn vm_runtime_primitive_benchmark_operations_are_executable() {
    measure_vm_process_runtime_primitives().expect("process primitive benchmark");
    measure_vm_scheduler_runtime_primitives().expect("scheduler primitive benchmark");
    measure_vm_resource_runtime_primitives().expect("resource primitive benchmark");
    measure_vm_cancellation_resource_cleanup_primitives()
        .expect("cancellation resource cleanup primitive benchmark");
    measure_vm_table_runtime_primitives().expect("table primitive benchmark");
    run_vm_map_workload(16).expect("VM map benchmark size 16");
    run_vm_map_workload(32).expect("VM map benchmark size 32");
    run_vm_map_workload(33).expect("VM map benchmark size 33");
    run_vm_map_workload(128).expect("VM map benchmark size 128");
    measure_vm_actor_runtime_primitives().expect("actor primitive benchmark");
    measure_vm_selective_receive_primitives().expect("selective receive primitive benchmark");
    measure_vm_timer_runtime_primitives().expect("timer primitive benchmark");
}

/// Verifies map benchmark names cover the OTP threshold pressure points.
///
/// Inputs:
/// - Static map benchmark sizes.
///
/// Output:
/// - Test passes when row names remain split by Terlan VM and OTP.
///
/// Transformation:
/// - Protects the benchmark from losing the 32-to-33-entry comparison that
///   motivates the VM adaptive-map work.
#[test]
pub(super) fn map_benchmark_tracks_cover_otp_threshold_sizes() {
    assert_eq!(MAP_BENCHMARK_SIZES, &[16, 32, 33, 127, 128, 129, 5_000]);
    assert_eq!(
        MAP_BENCHMARK_SIZES
            .iter()
            .map(|size| vm_map_measurement_name(*size))
            .collect::<Vec<_>>(),
        vec![
            "terlan_vm_map_insert_lookup_update_size_16",
            "terlan_vm_map_insert_lookup_update_size_32",
            "terlan_vm_map_insert_lookup_update_size_33",
            "terlan_vm_map_insert_lookup_update_size_127",
            "terlan_vm_map_insert_lookup_update_size_128",
            "terlan_vm_map_insert_lookup_update_size_129",
            "terlan_vm_map_insert_lookup_update_size_5000",
        ]
    );
    assert_eq!(
        MAP_BENCHMARK_SIZES
            .iter()
            .map(|size| otp_map_measurement_name(*size))
            .collect::<Vec<_>>(),
        vec![
            "otp_map_insert_lookup_update_size_16",
            "otp_map_insert_lookup_update_size_32",
            "otp_map_insert_lookup_update_size_33",
            "otp_map_insert_lookup_update_size_127",
            "otp_map_insert_lookup_update_size_128",
            "otp_map_insert_lookup_update_size_129",
            "otp_map_insert_lookup_update_size_5000",
        ]
    );
}

#[test]
pub(super) fn large_map_reference_lane_requires_vm_to_beat_otp() {
    let winning = vec![
        test_measurement(vm_map_measurement_name(5_000), 10),
        test_measurement(otp_map_measurement_name(5_000), 20),
    ];
    assert!(assert_vm_wins_large_map_reference_lane(&winning).is_ok());

    let losing = vec![
        test_measurement(vm_map_measurement_name(5_000), 20),
        test_measurement(otp_map_measurement_name(5_000), 10),
    ];
    let error = assert_vm_wins_large_map_reference_lane(&losing)
        .expect_err("OTP win should fail the large-map reference lane");
    assert!(error.contains("must beat OTP"));
}

#[cfg(test)]
pub(super) fn test_measurement(name: &'static str, mean_ns: u128) -> Measurement {
    Measurement {
        name,
        unit: "nanoseconds",
        iterations: 1,
        total_ns: mean_ns,
        min_ns: mean_ns,
        mean_ns,
        p50_ns: mean_ns,
        p95_ns: mean_ns,
        p99_ns: mean_ns,
        max_ns: mean_ns,
        total_us: mean_ns / 1_000,
        min_us: mean_ns / 1_000,
        mean_us: mean_ns / 1_000,
        p50_us: mean_ns / 1_000,
        p95_us: mean_ns / 1_000,
        p99_us: mean_ns / 1_000,
        max_us: mean_ns / 1_000,
    }
}

/// Verifies generated OTP map benchmark source uses native maps.
///
/// Inputs:
/// - One threshold-sized benchmark eval expression.
///
/// Output:
/// - Test passes when the expression keeps internal timing and correctness
///   assertions inside OTP.
///
/// Transformation:
/// - Avoids shelling out to Erlang during unit tests while still locking
///   down the source emitted by the benchmark gate.
#[test]
pub(super) fn otp_map_benchmark_eval_uses_native_map_assertions() {
    let source = otp_map_benchmark_eval(33, 3);

    assert!(source.contains("Size = 33"));
    assert!(source.contains("Iterations = 3"));
    assert!(source.contains("maps:put"));
    assert!(source.contains("maps:get"));
    assert!(source.contains("maps:update"));
    assert!(source.contains("erlang:monotonic_time(nanosecond)"));
    assert!(source.contains("{true, true} -> End - Start"));
}

/// Verifies OTP duration parsing accepts only the expected list shape.
///
/// Inputs:
/// - Stable Erlang list text and malformed text.
///
/// Output:
/// - Test passes when valid output becomes durations and invalid output is
///   rejected.
///
/// Transformation:
/// - Keeps the OTP comparison parser deterministic for benchmark gates.
#[test]
pub(super) fn otp_map_duration_list_parser_is_strict() {
    let durations =
        parse_erlang_duration_list("[1, 2000, 3000000]").expect("duration list should parse");

    assert_eq!(
        durations,
        vec![
            Duration::from_nanos(1),
            Duration::from_nanos(2_000),
            Duration::from_nanos(3_000_000),
        ]
    );
    assert!(parse_erlang_duration_list("{1,2,3}").is_err());
}
