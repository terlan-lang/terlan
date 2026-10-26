//! Paired lifecycle measurements of owned text fragments and managed reference batches.

use std::hint::black_box;
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::*;

#[derive(Debug, Serialize)]
struct Profile {
    fragments: usize,
    representation: &'static str,
    samples: usize,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
    managed_bytes_before_collection: usize,
    objects_before_collection: usize,
    retained_output_bytes: usize,
    modeled_scratch_metadata_bytes: usize,
    modeled_scratch_text_bytes: usize,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    native_revision: &'static str,
    build_policy: &'static str,
    source_sha256: String,
    build_profile: &'static str,
    os: &'static str,
    architecture: &'static str,
    scope: &'static str,
    memory_scope: &'static str,
    correctness_verified: bool,
    rows: Vec<Profile>,
}

/// Includes input materialization, list construction, joining, GC, verification and drop.
fn sample(fragments: &[String], compact: bool) -> (u128, usize, usize, usize) {
    let started = Instant::now();
    let (mut heap, layouts, descriptor) = fixture();
    let values = fragments
        .iter()
        .map(|text| {
            ManagedFieldValue::Reference(heap.allocate_string(black_box(text)).unwrap().erase())
        })
        .collect::<Vec<_>>();
    let list = heap.list_from_elements(&descriptor, &values).unwrap();
    drop(values);
    let result = if compact {
        join_string_list(&mut heap, &layouts, list.encoded_abi_word() as i64).unwrap()
    } else {
        // Previous production representation, retained here as the explicit control.
        let values = heap.list_elements(&descriptor, list).unwrap();
        let owned = values
            .into_iter()
            .map(|value| {
                let ManagedFieldValue::Reference(reference) = value else {
                    panic!("string reference")
                };
                heap.read_string(reference.cast()).unwrap().to_owned()
            })
            .collect::<Vec<_>>();
        let capacity = owned.iter().map(String::len).sum();
        let mut joined = String::with_capacity(capacity);
        for fragment in owned {
            joined.push_str(&fragment);
        }
        heap.allocate_string(&joined).unwrap()
    };
    let managed = heap.allocated_bytes();
    let objects = heap.object_count();
    let retained = heap.retained_size(result).unwrap();
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        result.erase(),
    )];
    heap.collect(&mut roots, usize::MAX).unwrap();
    let actual = heap.read_string(roots[0].reference().cast()).unwrap();
    assert!(actual
        .as_bytes()
        .iter()
        .copied()
        .eq(fragments.iter().flat_map(|text| text.bytes())));
    black_box(actual);
    drop(heap);
    drop(layouts);
    (started.elapsed().as_nanos(), managed, objects, retained)
}

#[test]
fn paired_string_join_profiles_report_memory_and_tail_latency() {
    let mut rows = Vec::new();
    for count in [8, 128, 1025] {
        let fragments = (0..count)
            .map(|index| format!("fragment-{index:016}-λ"))
            .collect::<Vec<_>>();
        let text_bytes = fragments.iter().map(String::len).sum::<usize>();
        let mut samples = [Vec::new(), Vec::new()];
        let mut metrics = [(0, 0, 0); 2];
        // Warm each lane, then alternate which representation executes first.
        sample(&fragments, false);
        sample(&fragments, true);
        for pair in 0..32 {
            for lane in [pair % 2, 1 - pair % 2] {
                let (duration, bytes, objects, retained) = sample(&fragments, lane == 1);
                samples[lane].push(duration);
                metrics[lane] = (bytes, objects, retained);
            }
        }
        for (lane, mut durations) in samples.into_iter().enumerate() {
            durations.sort_unstable();
            let percentile =
                |percent: usize| durations[((durations.len() - 1) * percent).div_ceil(100)];
            rows.push(Profile {
                fragments: count,
                representation: if lane == 1 {
                    "managed-reference-batch"
                } else {
                    "owned-fragments-control"
                },
                samples: durations.len(),
                p50_ns: percentile(50),
                p95_ns: percentile(95),
                p99_ns: percentile(99),
                max_ns: *durations.last().unwrap(),
                managed_bytes_before_collection: metrics[lane].0,
                objects_before_collection: metrics[lane].1,
                retained_output_bytes: metrics[lane].2,
                modeled_scratch_metadata_bytes: count
                    * if lane == 1 {
                        std::mem::size_of::<TvmRef<ManagedString>>()
                            + std::mem::size_of::<&str>()
                            + if text_bytes < 1024 {
                                std::mem::size_of::<std::ops::Range<usize>>()
                            } else {
                                0
                            }
                    } else {
                        std::mem::size_of::<String>() + std::mem::size_of::<ManagedFieldValue>()
                    },
                modeled_scratch_text_bytes: if lane == 1 {
                    usize::from(text_bytes >= 1024) * text_bytes
                } else {
                    text_bytes * 2
                },
            });
        }
    }
    let report = Report {
        schema: "terlan.managed-string-join-profiles.v1",
        native_revision: env!("TERLAN_NATIVE_CODEGEN_REVISION_SHA256"),
        build_policy: env!("TERLAN_NATIVE_BUILD_POLICY_SHA256"),
        source_sha256: source_fingerprint(),
        build_profile: if cfg!(debug_assertions) { "debug" } else { "release" },
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        scope: "materialize inputs; build list; join; collect; verify exact bytes; drop heap and registry; input generation excluded",
        memory_scope: "managed semispace and retained output are measured; scratch is modeled logical storage excluding allocator overhead and capacity rounding; no RSS claim",
        correctness_verified: true, rows,
    };
    if let Some(path) = std::env::var_os("TERLAN_STRING_JOIN_PROFILE_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert_eq!(report.rows.len(), 6);
    assert!(report
        .rows
        .iter()
        .all(|row| row.samples == 32 && row.retained_output_bytes > 0));
}

fn source_fingerprint() -> String {
    let mut digest = Sha256::new();
    for source in [
        include_str!("string.rs"),
        include_str!("../sequences.rs"),
        include_str!("../lists/construction.rs"),
        include_str!("../lists/traversal.rs"),
        include_str!("string_join_profile_test.rs"),
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")),
    ] {
        digest.update((source.len() as u64).to_le_bytes());
        digest.update(source.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
