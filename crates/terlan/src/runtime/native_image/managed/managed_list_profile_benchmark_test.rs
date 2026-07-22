use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;

use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits};

const LOOKUP_SAMPLES: usize = 512;
const MUTATION_SAMPLES: usize = 128;
const CONCAT_SAMPLES: usize = 64;
const BUILD_SAMPLES: usize = 16;

/// Stable timing summary for one managed-list operation profile.
#[derive(Debug, Serialize)]
struct OperationProfile {
    name: &'static str,
    samples: usize,
    p50_ns: u128,
    p95_ns: u128,
    max_ns: u128,
    objects_per_operation_max: usize,
}

/// Versioned managed-list profile benchmark artifact.
#[derive(Debug, Serialize)]
struct ManagedListProfileReport {
    schema: &'static str,
    benchmark: &'static str,
    inline_limit: usize,
    branch_factor: usize,
    correctness_verified: bool,
    operations: Vec<OperationProfile>,
}

/// Creates one heap large enough to retain every benchmark sample.
fn benchmark_heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(301).expect("actor"),
        HeapLimits::new(8 * 1024 * 1024, 64 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

/// Creates ordered integer fields for deterministic benchmark fixtures.
fn benchmark_ints(start: usize, count: usize) -> Vec<ManagedFieldValue> {
    (start..start + count)
        .map(|value| ManagedFieldValue::Int(value as i64))
        .collect()
}

/// Converts raw samples and a structural allocation budget into one profile.
fn operation_profile(
    name: &'static str,
    mut samples: Vec<u128>,
    objects_per_operation_max: usize,
) -> OperationProfile {
    samples.sort_unstable();
    let p50_index = (samples.len() - 1) / 2;
    let p95_index = ((samples.len() - 1) * 95).div_ceil(100);
    OperationProfile {
        name,
        samples: samples.len(),
        p50_ns: samples[p50_index],
        p95_ns: samples[p95_index],
        max_ns: samples[samples.len() - 1],
        objects_per_operation_max,
    }
}

/// Measures one immutable lookup profile without changing heap shape.
fn benchmark_lookup(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    index: usize,
    name: &'static str,
) -> OperationProfile {
    let mut samples = Vec::with_capacity(LOOKUP_SAMPLES);
    for _ in 0..LOOKUP_SAMPLES {
        let started = Instant::now();
        let value = heap.list_get(descriptor, black_box(list), black_box(index));
        black_box(value.expect("lookup"));
        samples.push(started.elapsed().as_nanos());
    }
    operation_profile(name, samples, 0)
}

/// Writes the benchmark report to its explicit or canonical quality path.
fn write_report(report: &ManagedListProfileReport) {
    let path = std::env::var_os("TERLAN_MANAGED_LIST_PROFILE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/quality/tvm-managed-list-profile.json"));
    fs::create_dir_all(path.parent().expect("report parent")).expect("create report directory");
    fs::write(
        &path,
        serde_json::to_vec_pretty(report).expect("serialize report"),
    )
    .expect("write report");
    println!("managed list profile report: {}", path.display());
}

/// Records timing baselines while enforcing deterministic RRB shape budgets.
#[test]
fn managed_list_profiles_emit_stable_benchmark_report() {
    let descriptor =
        ManagedListDescriptor::new("List[Int]", ManagedFieldType::Int).expect("descriptor");
    let mut heap = benchmark_heap();
    let inline = heap
        .list_from_elements(&descriptor, &benchmark_ints(0, 8))
        .expect("inline");
    let regular = heap
        .list_from_elements(&descriptor, &benchmark_ints(0, 1024))
        .expect("regular");
    let relaxed = heap
        .list_from_elements(&descriptor, &benchmark_ints(0, 1025))
        .expect("relaxed");
    assert_eq!(
        heap.list_profile(&descriptor, inline),
        Ok(ManagedListProfile::Inline)
    );
    assert_eq!(
        heap.list_profile(&descriptor, regular),
        Ok(ManagedListProfile::RegularTree)
    );
    assert_eq!(
        heap.list_profile(&descriptor, relaxed),
        Ok(ManagedListProfile::RelaxedTree)
    );

    let mut operations = vec![
        benchmark_lookup(&heap, &descriptor, inline, 7, "lookup.inline.8"),
        benchmark_lookup(&heap, &descriptor, regular, 1023, "lookup.regular.1024"),
        benchmark_lookup(&heap, &descriptor, relaxed, 1024, "lookup.relaxed.1025"),
    ];

    let mut update_samples = Vec::with_capacity(MUTATION_SAMPLES);
    let mut update_objects = 0;
    for index in 0..MUTATION_SAMPLES {
        let before = heap.object_count();
        let started = Instant::now();
        let updated = heap
            .list_update(
                &descriptor,
                black_box(regular),
                black_box(index * 7),
                ManagedFieldValue::Int(-1),
            )
            .expect("update");
        update_samples.push(started.elapsed().as_nanos());
        update_objects = update_objects.max(heap.object_count() - before);
        black_box(updated);
    }
    assert_eq!(update_objects, 3);
    operations.push(operation_profile(
        "update.regular.1024",
        update_samples,
        update_objects,
    ));

    let mut append_samples = Vec::with_capacity(MUTATION_SAMPLES);
    let mut append_objects = 0;
    for _ in 0..MUTATION_SAMPLES {
        let before = heap.object_count();
        let started = Instant::now();
        let appended = heap
            .list_append(
                &descriptor,
                black_box(regular),
                ManagedFieldValue::Int(1024),
            )
            .expect("append");
        append_samples.push(started.elapsed().as_nanos());
        append_objects = append_objects.max(heap.object_count() - before);
        black_box(appended);
    }
    assert!(append_objects <= 4);
    operations.push(operation_profile(
        "append.full.1024",
        append_samples,
        append_objects,
    ));

    let right = heap
        .list_from_elements(&descriptor, &benchmark_ints(1024, 1024))
        .expect("right");
    let mut concat_samples = Vec::with_capacity(CONCAT_SAMPLES);
    let mut concat_objects = 0;
    for _ in 0..CONCAT_SAMPLES {
        let before = heap.object_count();
        let started = Instant::now();
        let concatenated = heap
            .list_concat(&descriptor, black_box(regular), black_box(right))
            .expect("concat");
        concat_samples.push(started.elapsed().as_nanos());
        concat_objects = concat_objects.max(heap.object_count() - before);
        black_box(concatenated);
    }
    assert!(concat_objects <= 6);
    operations.push(operation_profile(
        "concat.regular.1024x1024",
        concat_samples,
        concat_objects,
    ));

    let build_values = benchmark_ints(0, 2050);
    let mut build_samples = Vec::with_capacity(BUILD_SAMPLES);
    let mut build_objects = 0;
    for _ in 0..BUILD_SAMPLES {
        let before = heap.object_count();
        let started = Instant::now();
        let mut builder = heap
            .list_builder(&descriptor, build_values.len())
            .expect("builder");
        builder
            .extend_from_slice(black_box(&build_values))
            .expect("extend");
        let built = builder.finish().expect("finish");
        build_samples.push(started.elapsed().as_nanos());
        build_objects = build_objects.max(heap.object_count() - before);
        black_box(built);
    }
    assert_eq!(build_objects, 70);
    operations.push(operation_profile(
        "build.transient.2050",
        build_samples,
        build_objects,
    ));

    write_report(&ManagedListProfileReport {
        schema: "terlan.tvm.managed-list-profile.v1",
        benchmark: "managed-list-profile",
        inline_limit: INLINE_LIMIT,
        branch_factor: BRANCH_FACTOR,
        correctness_verified: true,
        operations,
    });
}
