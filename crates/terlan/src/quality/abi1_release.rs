use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::terlan_quality::QualityResult;

const EVIDENCE_SCHEMA: &str = "terlan.abi1.gate-evidence.v1";
const REPORT_SCHEMA: &str = "terlan.abi1.gate-report.v1";
const ABI_VERSION: u64 = 1;
const MANAGED_LAYOUT_PROFILE: u64 = 1;
const MAX_TAIL_P95_NS: u64 = 100_000;
const MAX_TAIL_P99_NS: u64 = 200_000;

const PREREQUISITE_GATES: &[Abi1ReleaseGate] = &[
    Abi1ReleaseGate::ContinuousFuzz,
    Abi1ReleaseGate::CrossTargetConformance,
    Abi1ReleaseGate::TailLatency,
    Abi1ReleaseGate::ZeroCopyConformance,
    Abi1ReleaseGate::SpecializationEquivalence,
    Abi1ReleaseGate::TrustedAdapterAudit,
];

/// One executable ABI 1 optimization or compatibility release gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abi1ReleaseGate {
    /// Deterministic, seeded ABI mutation and malformed-input fuzz evidence.
    ContinuousFuzz,
    /// ABI conformance evidence from multiple CPU architectures.
    CrossTargetConformance,
    /// Measured p95 and p99 latency evidence checked against explicit limits.
    TailLatency,
    /// Checked zero-copy implementation and behavioral-test ownership.
    ZeroCopyConformance,
    /// Generic and specialized execution-result equivalence evidence.
    SpecializationEquivalence,
    /// Audit proving trusted in-process adapters have not bypassed isolation.
    TrustedAdapterAudit,
    /// Aggregate candidate requiring every optimization gate report.
    ReleaseCandidate,
    /// Explicit compatibility freeze bound to a validated release candidate.
    CompatibilityFreeze,
}

impl Abi1ReleaseGate {
    /// Resolves one quality CLI command to its ABI 1 gate.
    pub fn from_command(command: &str) -> Option<Self> {
        Some(match command {
            "abi1-continuous-fuzz" => Self::ContinuousFuzz,
            "abi1-cross-target-conformance" => Self::CrossTargetConformance,
            "abi1-tail-latency" => Self::TailLatency,
            "abi1-zero-copy-conformance" => Self::ZeroCopyConformance,
            "abi1-specialization-equivalence" => Self::SpecializationEquivalence,
            "abi1-trusted-adapter-audit" => Self::TrustedAdapterAudit,
            "abi1-release-candidate" => Self::ReleaseCandidate,
            "abi1-compatibility-freeze" => Self::CompatibilityFreeze,
            _ => return None,
        })
    }

    /// Stable gate identifier used in evidence and report documents.
    pub const fn id(self) -> &'static str {
        match self {
            Self::ContinuousFuzz => "continuous-fuzz",
            Self::CrossTargetConformance => "cross-target-conformance",
            Self::TailLatency => "tail-latency",
            Self::ZeroCopyConformance => "zero-copy-conformance",
            Self::SpecializationEquivalence => "specialization-equivalence",
            Self::TrustedAdapterAudit => "trusted-adapter-audit",
            Self::ReleaseCandidate => "release-candidate",
            Self::CompatibilityFreeze => "compatibility-freeze",
        }
    }

    fn evidence_path(self) -> PathBuf {
        PathBuf::from(format!("target/abi1-evidence/{}.json", self.id()))
    }

    fn report_path(self) -> PathBuf {
        PathBuf::from(format!("target/quality/abi1-{}-report.json", self.id()))
    }
}

/// Summary produced by an ABI 1 optimization or compatibility gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abi1ReleaseGateSummary {
    /// Stable gate identifier.
    pub gate: &'static str,
    /// Number of measured runs, implementation cases, or prerequisite reports checked.
    pub case_count: usize,
    /// Normalized report emitted by the gate.
    pub report_path: PathBuf,
}

struct GateValidation {
    case_count: usize,
    revision: Option<String>,
}

/// Runs one ABI 1 optimization or compatibility gate.
pub fn run_abi1_release_gate(
    root: &Path,
    gate: Abi1ReleaseGate,
) -> QualityResult<Abi1ReleaseGateSummary> {
    let validation = match gate {
        Abi1ReleaseGate::ContinuousFuzz
        | Abi1ReleaseGate::CrossTargetConformance
        | Abi1ReleaseGate::TailLatency
        | Abi1ReleaseGate::SpecializationEquivalence => validate_external_evidence(root, gate)?,
        Abi1ReleaseGate::ZeroCopyConformance => GateValidation {
            case_count: validate_zero_copy(root)?,
            revision: None,
        },
        Abi1ReleaseGate::TrustedAdapterAudit => GateValidation {
            case_count: validate_trusted_adapters(root)?,
            revision: None,
        },
        Abi1ReleaseGate::ReleaseCandidate => validate_release_candidate(root)?,
        Abi1ReleaseGate::CompatibilityFreeze => validate_compatibility_freeze(root)?,
    };
    let report_path = gate.report_path();
    write_report(
        &root.join(&report_path),
        json!({
            "schema": REPORT_SCHEMA,
            "gate": gate.id(),
            "abi_version": ABI_VERSION,
            "managed_layout_profile": MANAGED_LAYOUT_PROFILE,
            "status": "validated",
            "case_count": validation.case_count,
            "revision": validation.revision,
        }),
    )?;
    Ok(Abi1ReleaseGateSummary {
        gate: gate.id(),
        case_count: validation.case_count,
        report_path,
    })
}

fn validate_external_evidence(root: &Path, gate: Abi1ReleaseGate) -> QualityResult<GateValidation> {
    let path = root.join(gate.evidence_path());
    let document = read_json(&path, "ABI 1 gate evidence")?;
    require_string(&document, "schema", EVIDENCE_SCHEMA, gate)?;
    require_string(&document, "gate", gate.id(), gate)?;
    require_u64(&document, "abi_version", ABI_VERSION, gate)?;
    require_u64(
        &document,
        "managed_layout_profile",
        MANAGED_LAYOUT_PROFILE,
        gate,
    )?;
    require_string(&document, "status", "passed", gate)?;
    let revision = string_field(&document, "revision", gate)?;
    if revision.trim().is_empty() || revision == "unknown" {
        return fail(gate, "evidence revision must identify the tested source");
    }
    let runs = array_field(&document, "runs", gate)?;
    if runs.is_empty() {
        return fail(gate, "evidence must contain at least one measured run");
    }
    match gate {
        Abi1ReleaseGate::ContinuousFuzz => validate_fuzz_runs(runs, gate)?,
        Abi1ReleaseGate::CrossTargetConformance => validate_target_runs(runs, gate)?,
        Abi1ReleaseGate::TailLatency => validate_latency_runs(runs, gate)?,
        Abi1ReleaseGate::SpecializationEquivalence => validate_specialization_runs(runs, gate)?,
        _ => {
            return fail(
                gate,
                "internal error: gate does not accept external evidence",
            )
        }
    }
    Ok(GateValidation {
        case_count: runs.len(),
        revision: Some(revision.to_owned()),
    })
}

fn validate_fuzz_runs(runs: &[Value], gate: Abi1ReleaseGate) -> QualityResult<()> {
    let mut seeds = BTreeSet::new();
    let mut total_cases = 0u64;
    for run in runs {
        let seed = required_u64(run, "seed", gate)?;
        let cases = required_u64(run, "cases", gate)?;
        let failures = required_u64(run, "failures", gate)?;
        let digest = string_field(run, "corpus_digest", gate)?;
        if cases == 0 || failures != 0 || !is_sha256(digest) {
            return fail(
                gate,
                "each fuzz run requires cases > 0, failures = 0, and a SHA-256 corpus_digest",
            );
        }
        if !seeds.insert(seed) {
            return fail(gate, "fuzz seeds must be unique");
        }
        total_cases = total_cases.saturating_add(cases);
    }
    if seeds.len() < 3 || total_cases < 10_000 {
        return fail(
            gate,
            "continuous fuzz evidence requires at least 3 seeds and 10000 total cases",
        );
    }
    Ok(())
}

fn validate_target_runs(runs: &[Value], gate: Abi1ReleaseGate) -> QualityResult<()> {
    let mut targets = BTreeSet::new();
    let mut architectures = BTreeSet::new();
    for run in runs {
        let target = string_field(run, "target", gate)?;
        let architecture = string_field(run, "architecture", gate)?;
        if target.is_empty()
            || architecture.is_empty()
            || required_u64(run, "pointer_width", gate)? != 64
            || string_field(run, "endian", gate)? != "little"
            || required_u64(run, "failures", gate)? != 0
            || string_field(run, "status", gate)? != "passed"
        {
            return fail(
                gate,
                "every target must pass with a 64-bit little-endian ABI and zero failures",
            );
        }
        targets.insert(target);
        architectures.insert(architecture);
    }
    if targets.len() < 2 || !architectures.contains("x86_64") || !architectures.contains("aarch64")
    {
        return fail(
            gate,
            "cross-target evidence requires distinct x86_64 and aarch64 targets",
        );
    }
    Ok(())
}

fn validate_latency_runs(runs: &[Value], gate: Abi1ReleaseGate) -> QualityResult<()> {
    for run in runs {
        let samples = required_u64(run, "samples", gate)?;
        let p95 = required_u64(run, "p95_ns", gate)?;
        let p99 = required_u64(run, "p99_ns", gate)?;
        let p95_limit = required_u64(run, "p95_limit_ns", gate)?;
        let p99_limit = required_u64(run, "p99_limit_ns", gate)?;
        if string_field(run, "workload", gate)?.is_empty()
            || samples < 1_000
            || p95 == 0
            || p95 > p99
            || p95_limit > MAX_TAIL_P95_NS
            || p99_limit > MAX_TAIL_P99_NS
            || p95 > p95_limit
            || p99 > p99_limit
        {
            return fail(gate, "each latency workload requires 1000 samples, ordered nonzero tails, and p95/p99 within repository-owned limits");
        }
    }
    Ok(())
}

fn validate_specialization_runs(runs: &[Value], gate: Abi1ReleaseGate) -> QualityResult<()> {
    for run in runs {
        let case = string_field(run, "semantic_case", gate)?;
        let generic = string_field(run, "generic_digest", gate)?;
        let specialized = string_field(run, "specialized_digest", gate)?;
        if case.is_empty()
            || !is_sha256(generic)
            || generic != specialized
            || string_field(run, "generic_status", gate)? != "passed"
            || string_field(run, "specialized_status", gate)? != "passed"
        {
            return fail(gate, "generic and specialized executions must pass with identical SHA-256 result digests");
        }
    }
    Ok(())
}

fn validate_zero_copy(root: &Path) -> QualityResult<usize> {
    let gate = Abi1ReleaseGate::ZeroCopyConformance;
    let owners = [
        (
            "crates/terlan/src/runtime/native_image/managed/sequences.rs",
            &[
                "Borrowed semantic view of a managed bitstring slice",
                "Returns a zero-copy byte slice when both boundaries are byte aligned",
                "self.storage.get(start..end)",
            ][..],
        ),
        (
            "crates/terlan/src/runtime/native_image/managed/managed_sequence_test.rs",
            &[
                "binary_slices_enforce_bounds_and_bit_order",
                "sequence_graph_survives_precise_relocation",
                "typed_sequence_access_rejects_wrong_and_foreign_references",
            ][..],
        ),
    ];
    let mut checked = 0;
    for (relative, markers) in owners {
        let path = root.join(relative);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("[{}] {}: {error}", gate.id(), path.display()))?;
        for marker in markers {
            if !text.contains(marker) {
                return fail(gate, &format!("{relative}: missing `{marker}`"));
            }
            checked += 1;
        }
    }
    Ok(checked)
}

fn validate_trusted_adapters(root: &Path) -> QualityResult<usize> {
    let gate = Abi1ReleaseGate::TrustedAdapterAudit;
    let roots = [
        "crates/terlan/src/runtime/native_boundary",
        "crates/terlan/src/runtime/vm/native_boundary",
        "crates/terlan/src/runtime/vm/capability_worker",
        "crates/terlan/src/native_worker",
    ];
    let forbidden = [
        "unsafe {",
        "unsafe fn",
        "unsafe impl",
        "extern \"C\"",
        "trusted_in_shard",
        "trusted-in-shard",
    ];
    let mut files = Vec::new();
    for relative in roots {
        collect_rust_files(&root.join(relative), &mut files)
            .map_err(|error| format!("[{}] failed to inventory {relative}: {error}", gate.id()))?;
    }
    if files.is_empty() {
        return fail(
            gate,
            "native adapter audit found no Rust implementation files",
        );
    }
    for path in &files {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("[{}] {}: {error}", gate.id(), path.display()))?;
        for fragment in forbidden {
            if text.contains(fragment) {
                return fail(
                    gate,
                    &format!("{} contains forbidden `{fragment}`", path.display()),
                );
            }
        }
    }
    Ok(files.len())
}

fn validate_release_candidate(root: &Path) -> QualityResult<GateValidation> {
    let gate = Abi1ReleaseGate::ReleaseCandidate;
    let mut measured_revision = None;
    for prerequisite in PREREQUISITE_GATES {
        let path = root.join(prerequisite.report_path());
        let report = read_json(&path, "ABI 1 prerequisite report")?;
        require_string(&report, "schema", REPORT_SCHEMA, gate)?;
        require_string(&report, "gate", prerequisite.id(), gate)?;
        require_u64(&report, "abi_version", ABI_VERSION, gate)?;
        require_u64(
            &report,
            "managed_layout_profile",
            MANAGED_LAYOUT_PROFILE,
            gate,
        )?;
        require_string(&report, "status", "validated", gate)?;
        if matches!(
            prerequisite,
            Abi1ReleaseGate::ContinuousFuzz
                | Abi1ReleaseGate::CrossTargetConformance
                | Abi1ReleaseGate::TailLatency
                | Abi1ReleaseGate::SpecializationEquivalence
        ) {
            let revision = string_field(&report, "revision", gate)?;
            if revision.is_empty() {
                return fail(gate, "measured prerequisite revision is empty");
            }
            if measured_revision
                .as_deref()
                .is_some_and(|expected| expected != revision)
            {
                return fail(
                    gate,
                    "measured prerequisite reports identify different revisions",
                );
            }
            measured_revision = Some(revision.to_owned());
        }
    }
    Ok(GateValidation {
        case_count: PREREQUISITE_GATES.len(),
        revision: measured_revision,
    })
}

fn validate_compatibility_freeze(root: &Path) -> QualityResult<GateValidation> {
    let gate = Abi1ReleaseGate::CompatibilityFreeze;
    let candidate_path = root.join(Abi1ReleaseGate::ReleaseCandidate.report_path());
    let candidate = read_json(&candidate_path, "ABI 1 release candidate report")?;
    require_string(&candidate, "schema", REPORT_SCHEMA, gate)?;
    require_string(&candidate, "gate", "release-candidate", gate)?;
    require_string(&candidate, "status", "validated", gate)?;
    let candidate_revision = string_field(&candidate, "revision", gate)?;

    let baseline_path = root.join("docs/runtime/ABI1_COMPATIBILITY_BASELINE.json");
    let baseline = read_json(&baseline_path, "ABI 1 compatibility baseline")?;
    require_string(
        &baseline,
        "schema",
        "terlan.abi1.compatibility-baseline.v1",
        gate,
    )?;
    require_string(&baseline, "status", "frozen", gate)?;
    require_u64(&baseline, "abi_version", ABI_VERSION, gate)?;
    require_u64(
        &baseline,
        "managed_layout_profile",
        MANAGED_LAYOUT_PROFILE,
        gate,
    )?;
    require_string(&baseline, "release_revision", candidate_revision, gate)?;
    let terms = array_field(&baseline, "contract_terms", gate)?;
    if terms.is_empty() {
        return fail(gate, "frozen baseline must contain contract_terms");
    }
    let spec_path = root.join("docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md");
    let spec = fs::read_to_string(&spec_path)
        .map_err(|error| format!("[{}] {}: {error}", gate.id(), spec_path.display()))?;
    for term in terms {
        let term = term
            .as_str()
            .ok_or_else(|| format!("[{}] contract_terms must be strings", gate.id()))?;
        if term.is_empty() || !spec.contains(term) {
            return fail(gate, &format!("frozen contract term is absent: `{term}`"));
        }
    }
    Ok(GateValidation {
        case_count: terms.len(),
        revision: Some(candidate_revision.to_owned()),
    })
}

fn collect_rust_files(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_rust_files(&entry.path(), files)?;
        } else if file_type.is_file() && entry.path().extension().is_some_and(|ext| ext == "rs") {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn read_json(path: &Path, description: &str) -> QualityResult<Value> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("{}: failed to read {description}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{}: invalid {description}: {error}", path.display()))
}

fn write_report(path: &Path, report: Value) -> QualityResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: report has no parent directory", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "{}: failed to create report directory: {error}",
            parent.display()
        )
    })?;
    let text = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("failed to serialize ABI 1 gate report: {error}"))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("{}: failed to write report: {error}", path.display()))
}

fn string_field<'a>(
    document: &'a Value,
    field: &str,
    gate: Abi1ReleaseGate,
) -> QualityResult<&'a str> {
    document
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("[{}] `{field}` must be a string", gate.id()))
}

fn required_u64(document: &Value, field: &str, gate: Abi1ReleaseGate) -> QualityResult<u64> {
    document
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("[{}] `{field}` must be an unsigned integer", gate.id()))
}

fn array_field<'a>(
    document: &'a Value,
    field: &str,
    gate: Abi1ReleaseGate,
) -> QualityResult<&'a [Value]> {
    document
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("[{}] `{field}` must be an array", gate.id()))
}

fn require_string(
    document: &Value,
    field: &str,
    expected: &str,
    gate: Abi1ReleaseGate,
) -> QualityResult<()> {
    let actual = string_field(document, field, gate)?;
    if actual != expected {
        return fail(
            gate,
            &format!("`{field}` must be `{expected}`, found `{actual}`"),
        );
    }
    Ok(())
}

fn require_u64(
    document: &Value,
    field: &str,
    expected: u64,
    gate: Abi1ReleaseGate,
) -> QualityResult<()> {
    let actual = required_u64(document, field, gate)?;
    if actual != expected {
        return fail(
            gate,
            &format!("`{field}` must be {expected}, found {actual}"),
        );
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn fail<T>(gate: Abi1ReleaseGate, message: &str) -> QualityResult<T> {
    Err(format!("[{}] {message}", gate.id()))
}

#[cfg(test)]
#[path = "abi1_release_test.rs"]
mod abi1_release_test;
