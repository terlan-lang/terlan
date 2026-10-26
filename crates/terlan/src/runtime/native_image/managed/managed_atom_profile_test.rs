//! Paired construction, lookup, clone and destruction of two finite atom layouts.

use std::hint::black_box;
use std::time::Instant;

use serde::Serialize;

use super::*;

#[derive(Serialize)]
struct Profile {
    atoms: usize,
    representation: &'static str,
    samples: usize,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
    retained_text_bytes: usize,
    retained_index_bytes: usize,
    retained_buffers: usize,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    native_revision: &'static str,
    build_policy: &'static str,
    build_profile: &'static str,
    os: &'static str,
    architecture: &'static str,
    scope: &'static str,
    memory_scope: &'static str,
    correctness_verified: bool,
    rows: Vec<Profile>,
}

fn sample(names: &[String], packed: bool) -> (u128, usize, usize, usize) {
    let started = Instant::now();
    let (text_bytes, index_bytes, buffers) = if packed {
        let table = AtomTable::new(names.iter().rev().cloned()).unwrap();
        let cloned = table.clone();
        for (index, name) in names.iter().enumerate() {
            let atom = table.index(black_box(name)).unwrap();
            assert_eq!(atom.get(), index as u32);
            assert_eq!(cloned.identity(atom), Ok(name.as_str()));
        }
        let metrics = (
            table.text.len(),
            std::mem::size_of_val(table.ranges.as_ref()),
            2,
        );
        drop(cloned);
        drop(table);
        metrics
    } else {
        // The previous production table, with the same validation and ordering.
        let mut values = names.iter().rev().cloned().collect::<Vec<_>>();
        for value in &values {
            validate_atom_identity(value).unwrap();
        }
        values.sort_unstable();
        values.dedup();
        assert!(values.len() <= u32::MAX as usize);
        let table = values
            .into_iter()
            .map(String::into_boxed_str)
            .collect::<Box<[_]>>();
        let cloned = table.clone();
        for (index, name) in names.iter().enumerate() {
            validate_atom_identity(name).unwrap();
            let atom = table
                .binary_search_by(|value| value.as_ref().cmp(black_box(name)))
                .unwrap();
            assert_eq!(atom, index);
            assert_eq!(cloned[atom].as_ref(), name);
        }
        let metrics = (
            table.iter().map(|value| value.len()).sum(),
            std::mem::size_of_val(table.as_ref()),
            table.len() + 1,
        );
        drop(cloned);
        drop(table);
        metrics
    };
    (
        started.elapsed().as_nanos(),
        text_bytes,
        index_bytes,
        buffers,
    )
}

#[test]
fn paired_atom_table_profiles_report_storage_and_lifecycle() {
    let mut rows = Vec::new();
    for count in [8, 128, 1025] {
        let names = (0..count)
            .map(|index| format!("atom_{index:08}_λ"))
            .collect::<Vec<_>>();
        let mut timings = [Vec::new(), Vec::new()];
        let mut storage = [(0, 0, 0); 2];
        sample(&names, false);
        sample(&names, true);
        for pair in 0..32 {
            for lane in [pair % 2, 1 - pair % 2] {
                let (duration, text, index, buffers) = sample(&names, lane == 1);
                timings[lane].push(duration);
                storage[lane] = (text, index, buffers);
            }
        }
        assert_eq!(storage[0].0, storage[1].0);
        assert_eq!(storage[0].1, storage[1].1 * 2);
        assert_eq!(storage[1].2, 2);
        for (lane, mut samples) in timings.into_iter().enumerate() {
            samples.sort_unstable();
            let percentile =
                |percent: usize| samples[((samples.len() - 1) * percent).div_ceil(100)];
            rows.push(Profile {
                atoms: count,
                representation: if lane == 1 {
                    "packed-utf8-and-offsets"
                } else {
                    "owned-names-control"
                },
                samples: samples.len(),
                p50_ns: percentile(50),
                p95_ns: percentile(95),
                p99_ns: percentile(99),
                max_ns: *samples.last().unwrap(),
                retained_text_bytes: storage[lane].0,
                retained_index_bytes: storage[lane].1,
                retained_buffers: storage[lane].2,
            });
        }
    }
    let report = Report {
        schema: "terlan.atom-table-profiles.v1",
        native_revision: env!("TERLAN_NATIVE_CODEGEN_REVISION_SHA256"),
        build_policy: env!("TERLAN_NATIVE_BUILD_POLICY_SHA256"),
        build_profile: if cfg!(debug_assertions) { "debug" } else { "release" },
        os: std::env::consts::OS, architecture: std::env::consts::ARCH,
        scope: "clone input names; validate and sort; construct table; look up every name; clone table; verify every cloned index; destroy both tables; input generation excluded",
        memory_scope: "measured text/index storage and structural retained buffer count per table; excludes allocator overhead, temporary construction storage, and process RSS",
        correctness_verified: true, rows,
    };
    assert_eq!(report.rows.len(), 6);
    if let Some(path) = std::env::var_os("TERLAN_ATOM_TABLE_PROFILE_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
