use std::env;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use terlan::runtime::native_image::managed::{ActorHeap, ActorId, HeapLimits};
use terlan::runtime::native_image::{
    decode_descriptor, encode_descriptor, TvmBoundaryType, TvmCallableDescriptor,
    TvmExecutableDescriptor, TvmExportDescriptor, TvmImageIdentity, TvmImageIntegrity,
    TvmImageTarget,
};

const SCHEMA: &str = "terlan.abi1.gate-evidence.v1";

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn revision() -> String {
    let revision = env::var("TERLAN_ABI1_REVISION").expect("TERLAN_ABI1_REVISION is required");
    assert!(!revision.trim().is_empty() && revision != "unknown");
    revision
}

fn output_path(gate: &str) -> PathBuf {
    env::var_os("TERLAN_ABI1_EVIDENCE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("target/abi1-evidence/{gate}.json")))
}

fn write_json(path: &Path, document: &Value) {
    fs::create_dir_all(path.parent().expect("evidence parent")).expect("create evidence parent");
    let text = serde_json::to_string_pretty(document).expect("serialize ABI evidence");
    fs::write(path, format!("{text}\n")).expect("write ABI evidence");
}

fn envelope(gate: &str, runs: Vec<Value>) -> Value {
    json!({
        "schema": SCHEMA,
        "gate": gate,
        "abi_version": 1,
        "managed_layout_profile": 1,
        "status": "passed",
        "revision": revision(),
        "runs": runs,
    })
}

fn descriptor() -> TvmExecutableDescriptor {
    TvmExecutableDescriptor {
        runtime_abi_min: 3,
        runtime_abi_max: 3,
        native_boundary_min: 1,
        native_boundary_max: 1,
        target: TvmImageTarget {
            triple: "abi1-fuzz-target".to_owned(),
            architecture: env::consts::ARCH.to_owned(),
            operating_system: env::consts::OS.to_owned(),
            calling_convention: "c".to_owned(),
        },
        identity: TvmImageIdentity {
            compiler: "terlc-abi1-evidence".to_owned(),
            build: "deterministic".to_owned(),
            package: "abi1-evidence".to_owned(),
            module: "evidence.Main".to_owned(),
        },
        exports: vec![TvmExportDescriptor {
            id: 1,
            name: "identity/1".to_owned(),
            parameters: vec![TvmBoundaryType::Bytes],
            results: vec![TvmBoundaryType::Bytes],
        }],
        capabilities: Vec::new(),
        resources: Vec::new(),
        dependencies: Vec::new(),
        continuations: Vec::new(),
        callables: vec![TvmCallableDescriptor {
            id: 1,
            parameters: vec![TvmBoundaryType::Bytes],
            results: vec![TvmBoundaryType::Bytes],
            captures: Vec::new(),
        }],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: vec!["error".to_owned(), "ok".to_owned()],
        integrity: TvmImageIntegrity {
            code_digest: [7; 32],
            immutable_data_digest: [11; 32],
        },
        signature: None,
    }
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn abi1_continuous_fuzz_producer() {
    const SEEDS: [u64; 3] = [0x4152_4931, 0x5445_524c_414e, 0xd1ce_b00c];
    const CASES: u64 = 4_096;
    let canonical = encode_descriptor(&descriptor()).expect("canonical descriptor");
    let mut runs = Vec::new();

    for seed in SEEDS {
        let mut state = seed;
        let mut corpus = Sha256::new();
        for case in 0..CASES {
            let mut candidate = canonical.clone();
            let edits = 1 + (next_random(&mut state) % 4) as usize;
            for _ in 0..edits {
                let index = (next_random(&mut state) as usize) % candidate.len();
                candidate[index] ^= next_random(&mut state) as u8 | 1;
            }
            if case % 17 == 0 {
                let length = (next_random(&mut state) as usize) % candidate.len();
                candidate.truncate(length);
            }
            corpus.update((candidate.len() as u64).to_le_bytes());
            corpus.update(&candidate);
            let decoded = catch_unwind(AssertUnwindSafe(|| decode_descriptor(&candidate)))
                .expect("ABI descriptor decoder panicked on fuzz input");
            if let Ok(decoded) = decoded {
                let reencoded = encode_descriptor(&decoded).expect("decoded descriptor re-encodes");
                assert_eq!(
                    decode_descriptor(&reencoded).expect("canonical result decodes"),
                    decoded,
                    "successful fuzz decode was not canonical"
                );
            }
        }
        runs.push(json!({
            "seed": seed,
            "cases": CASES,
            "failures": 0,
            "corpus_digest": hex_digest(corpus.finalize()),
        }));
    }

    write_json(
        &output_path("continuous-fuzz"),
        &envelope("continuous-fuzz", runs),
    );
}

fn heap(owner: u64, hard_bytes: usize) -> ActorHeap {
    ActorHeap::new(
        ActorId::new(owner).expect("nonzero actor"),
        HeapLimits::new(hard_bytes / 2, hard_bytes).expect("heap limits"),
    )
    .expect("64-bit actor heap")
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = ((samples.len() - 1) * percentile).div_ceil(100);
    samples[index]
}

#[test]
fn abi1_tail_latency_producer() {
    const WARMUP: usize = 1_000;
    const SAMPLES: usize = 10_000;
    let payload = [0x5a; 64];
    let mut warmup = heap(700, 2 * 1024 * 1024);
    for _ in 0..WARMUP {
        let value = warmup.allocate_bytes(&payload).expect("warmup allocation");
        assert_eq!(warmup.read_bytes(value).expect("warmup read"), payload);
    }

    let mut measured = heap(701, 4 * 1024 * 1024);
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let value = measured
            .allocate_bytes(&payload)
            .expect("measured allocation");
        assert_eq!(measured.read_bytes(value).expect("measured read"), payload);
        samples.push(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    samples.sort_unstable();
    let p95 = percentile(&samples, 95);
    let p99 = percentile(&samples, 99);
    let p95_limit = env::var("TERLAN_ABI1_P95_LIMIT_NS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000);
    let p99_limit = env::var("TERLAN_ABI1_P99_LIMIT_NS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200_000);
    assert!(
        p95_limit <= 100_000,
        "p95 limit cannot weaken repository policy"
    );
    assert!(
        p99_limit <= 200_000,
        "p99 limit cannot weaken repository policy"
    );
    assert!(p95 <= p95_limit, "p95 {p95} exceeds limit {p95_limit}");
    assert!(p99 <= p99_limit, "p99 {p99} exceeds limit {p99_limit}");

    write_json(
        &output_path("tail-latency"),
        &envelope(
            "tail-latency",
            vec![json!({
                "workload": "actor-heap-bytes-allocate-read-64",
                "samples": SAMPLES,
                "p95_ns": p95,
                "p99_ns": p99,
                "p95_limit_ns": p95_limit,
                "p99_limit_ns": p99_limit,
            })],
        ),
    );
}

fn generic_binary_bytes(
    view: terlan::runtime::native_image::managed::ManagedBinaryView<'_>,
) -> Vec<u8> {
    assert!(view.bit_length().is_multiple_of(8));
    (0..view.bit_length() / 8)
        .map(|byte_index| {
            (0..8).fold(0u8, |byte, bit_index| {
                byte | (u8::from(view.bit(byte_index * 8 + bit_index).expect("bit"))
                    << (7 - bit_index))
            })
        })
        .collect()
}

#[test]
fn abi1_specialization_equivalence_producer() {
    let cases = [
        ("single-byte", vec![0xa5]),
        ("protocol-header", (0u8..32).collect()),
        (
            "page-fragment",
            (0..4096).map(|index| (index % 251) as u8).collect(),
        ),
    ];
    let mut runs = Vec::new();
    for (index, (name, payload)) in cases.into_iter().enumerate() {
        let mut heap = heap(800 + index as u64, payload.len() * 4 + 8192);
        let storage = heap.allocate_bytes(&payload).expect("binary storage");
        let binary = heap
            .allocate_binary(storage, 0, payload.len() * 8)
            .expect("aligned binary");
        let view = heap.read_binary(binary).expect("binary view");
        let generic = generic_binary_bytes(view);
        let specialized = view.aligned_bytes().expect("zero-copy aligned path");
        assert_eq!(generic, specialized);
        let generic_digest = hex_digest(Sha256::digest(&generic));
        let specialized_digest = hex_digest(Sha256::digest(specialized));
        runs.push(json!({
            "semantic_case": name,
            "generic_digest": generic_digest,
            "specialized_digest": specialized_digest,
            "generic_status": "passed",
            "specialized_status": "passed",
        }));
    }
    write_json(
        &output_path("specialization-equivalence"),
        &envelope("specialization-equivalence", runs),
    );
}

#[test]
fn abi1_cross_target_probe() {
    assert_eq!(usize::BITS, 64, "ABI 1 requires a 64-bit target");
    assert!(
        cfg!(target_endian = "little"),
        "ABI 1 requires little endian"
    );
    let architecture = env::consts::ARCH;
    assert!(matches!(architecture, "x86_64" | "aarch64"));
    let target =
        env::var("TERLAN_ABI1_TARGET_TRIPLE").expect("TERLAN_ABI1_TARGET_TRIPLE is required");
    assert!(target.starts_with(architecture));
    let path = env::var_os("TERLAN_ABI1_TARGET_FRAGMENT")
        .map(PathBuf::from)
        .expect("TERLAN_ABI1_TARGET_FRAGMENT is required");
    write_json(
        &path,
        &json!({
            "target": target,
            "architecture": architecture,
            "pointer_width": usize::BITS,
            "endian": if cfg!(target_endian = "little") { "little" } else { "big" },
            "failures": 0,
            "status": "passed",
        }),
    );
}
