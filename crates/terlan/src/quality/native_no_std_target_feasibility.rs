use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;

use super::QualityResult;

const REPORT_PATH: &str = "target/quality/native-no-std-target-feasibility-report.json";
const DIAGNOSTIC_CODE: &str = "native_target_unsupported_feature";
const REQUIRED_ADVERSARIAL_CASES: &[&str] = &[
    "ambient filesystem access",
    "ambient networking access",
    "implicit process access",
    "implicit heap allocation",
    "VM actor use on VM-free target",
    "blocking operation on constrained target",
    "NativeBoundary use without declared capability",
    "host runtime fallback",
];
const FUTURE_PREREQUISITES: &[&str] = &[
    "target declaration syntax and typechecker integration",
    "VM-free CoreIR native lowering",
    "reduced VM runtime profile",
    "explicit allocator and panic lowering",
    "package-maintained HAL capability manifests",
    "target-specific linker and artifact production",
];

/// Summary emitted by the native constrained-target feasibility gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNoStdTargetFeasibilitySummary {
    pub target_count: usize,
    pub feature_count: usize,
    pub rejected_feature_count: usize,
    pub adversarial_case_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SupportClass {
    NativeLowering,
    ReducedVm,
    HostOsRequired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FeatureSupport {
    feature: &'static str,
    support: SupportClass,
    required_capability: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TargetRow {
    target: &'static str,
    runtime: &'static str,
    rust_target_notes: &'static str,
    std_subset: &'static [&'static str],
    features: Vec<FeatureSupport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FixtureDiagnostic {
    code: &'static str,
    target: &'static str,
    feature: &'static str,
    required_capability: Option<&'static str>,
    line: usize,
}

/// Writes the deterministic capability matrix for native constrained targets.
///
/// This gate records feasibility only. It deliberately rejects unsupported
/// source instead of claiming that firmware, kernel, or `no_std` artifacts can
/// already be produced by the compiler.
pub fn run_native_no_std_target_feasibility(
    root: &Path,
) -> QualityResult<NativeNoStdTargetFeasibilitySummary> {
    let targets = target_matrix();
    validate_target_matrix(&targets)?;
    let accepted_fixture = validate_source_fixture(
        target(&targets, "bare-metal-no-std")?,
        "import std.core.Int.\npub add(a: Int, b: Int): Int -> a + b.",
    );
    if !accepted_fixture.is_empty() {
        return Err(render_failure(&[
            "pure constrained-target fixture was rejected".to_string(),
        ]));
    }

    let rejected_fixture = validate_source_fixture(
        target(&targets, "bare-metal-no-std")?,
        "import std.io.File.\nimport std.net.Tcp.\nimport std.vm.Actor.\n",
    );
    if rejected_fixture.len() != 3 {
        return Err(render_failure(&[format!(
            "constrained rejection fixture expected 3 diagnostics, found {}",
            rejected_fixture.len()
        )]));
    }

    let feature_count = targets
        .iter()
        .flat_map(|row| row.features.iter().map(|feature| feature.feature))
        .collect::<BTreeSet<_>>()
        .len();
    let rejected_features = targets
        .iter()
        .flat_map(|row| row.features.iter())
        .filter(|feature| feature.support == SupportClass::Rejected)
        .map(|feature| feature.feature)
        .collect::<BTreeSet<_>>();
    let report_path = root.join(REPORT_PATH);
    write_report(
        &report_path,
        &targets,
        &rejected_features,
        &rejected_fixture,
    )?;

    Ok(NativeNoStdTargetFeasibilitySummary {
        target_count: targets.len(),
        feature_count,
        rejected_feature_count: rejected_features.len(),
        adversarial_case_count: REQUIRED_ADVERSARIAL_CASES.len(),
        report_path,
    })
}

fn target_matrix() -> Vec<TargetRow> {
    vec![
        host_row(
            "native-host",
            "full-vm",
            "host Rust target selected by release artifact",
        ),
        host_row(
            "embedded-linux",
            "reduced-vm",
            "supported Linux Rust target selected by device profile",
        ),
        constrained_row(
            "rtos-like",
            "reduced-vm-planned",
            "custom Rust target and maintained RTOS package required",
            true,
            true,
        ),
        constrained_row(
            "bare-metal-no-std",
            "vm-free",
            "explicit `*-unknown-none*` Rust target required",
            false,
            false,
        ),
        constrained_row(
            "kernel-restricted",
            "vm-free",
            "custom kernel Rust target and panic policy required",
            false,
            false,
        ),
        constrained_row(
            "risc-v-soc",
            "vm-free-planned",
            "device profile must name a supported RISC-V ISA Rust target",
            false,
            true,
        ),
        constrained_row(
            "arm-microcontroller",
            "vm-free-planned",
            "device profile must name a supported ARM Cortex-M Rust target",
            false,
            true,
        ),
    ]
}

fn host_row(
    target: &'static str,
    runtime: &'static str,
    rust_target_notes: &'static str,
) -> TargetRow {
    TargetRow {
        target,
        runtime,
        rust_target_notes,
        std_subset: &["std.core", "std.collections", "std.vm", "std.io", "std.net"],
        features: feature_rows(
            SupportClass::ReducedVm,
            SupportClass::HostOsRequired,
            SupportClass::HostOsRequired,
        ),
    }
}

fn constrained_row(
    target: &'static str,
    runtime: &'static str,
    rust_target_notes: &'static str,
    supports_reduced_vm: bool,
    supports_declared_heap: bool,
) -> TargetRow {
    let mut features = feature_rows(
        if supports_reduced_vm {
            SupportClass::ReducedVm
        } else {
            SupportClass::Rejected
        },
        SupportClass::Rejected,
        SupportClass::Rejected,
    );
    set_support(
        &mut features,
        "heap",
        if supports_declared_heap {
            SupportClass::NativeLowering
        } else {
            SupportClass::Rejected
        },
        Some("target.heap"),
    );
    set_support(
        &mut features,
        "hardware-access",
        SupportClass::NativeLowering,
        Some("package.hal"),
    );
    TargetRow {
        target,
        runtime,
        rust_target_notes,
        std_subset: &["std.core", "std.numeric.fixed", "std.target"],
        features,
    }
}

fn feature_rows(
    actors: SupportClass,
    os_services: SupportClass,
    blocking: SupportClass,
) -> Vec<FeatureSupport> {
    vec![
        feature("pure-functions", SupportClass::NativeLowering, None),
        feature("fixed-size-numerics", SupportClass::NativeLowering, None),
        feature("static-data", SupportClass::NativeLowering, None),
        feature("heap", SupportClass::HostOsRequired, Some("target.heap")),
        feature("actors", actors, Some("runtime.vm")),
        feature("filesystem", os_services, Some("os.filesystem")),
        feature("networking", os_services, Some("os.networking")),
        feature("processes", os_services, Some("os.process")),
        feature("blocking-operations", blocking, Some("runtime.blocking")),
        feature(
            "native-boundary",
            os_services,
            Some("package.native-boundary"),
        ),
        feature(
            "hardware-access",
            SupportClass::HostOsRequired,
            Some("package.hal"),
        ),
        feature("ambient-runtime", SupportClass::Rejected, None),
    ]
}

fn feature(
    feature: &'static str,
    support: SupportClass,
    required_capability: Option<&'static str>,
) -> FeatureSupport {
    FeatureSupport {
        feature,
        support,
        required_capability,
    }
}

fn set_support(
    features: &mut [FeatureSupport],
    name: &str,
    support: SupportClass,
    required_capability: Option<&'static str>,
) {
    let feature = features
        .iter_mut()
        .find(|feature| feature.feature == name)
        .expect("built-in feature row");
    feature.support = support;
    feature.required_capability = required_capability;
}

fn validate_target_matrix(targets: &[TargetRow]) -> QualityResult<()> {
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    for row in targets {
        if !names.insert(row.target) {
            diagnostics.push(format!("duplicate target row `{}`", row.target));
        }
        let mut features = BTreeSet::new();
        for feature in &row.features {
            if !features.insert(feature.feature) {
                diagnostics.push(format!(
                    "target `{}` contains duplicate feature `{}`",
                    row.target, feature.feature
                ));
            }
        }
        if feature_support(row, "ambient-runtime") != Some(SupportClass::Rejected) {
            diagnostics.push(format!(
                "target `{}` permits an ambient runtime",
                row.target
            ));
        }
        if row.runtime == "vm-free"
            && feature_support(row, "actors") != Some(SupportClass::Rejected)
        {
            diagnostics.push(format!(
                "VM-free target `{}` permits actor execution",
                row.target
            ));
        }
        if row.std_subset.is_empty() || row.rust_target_notes.trim().is_empty() {
            diagnostics.push(format!(
                "target `{}` has incomplete target notes",
                row.target
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(render_failure(&diagnostics))
    }
}

fn validate_source_fixture(row: &TargetRow, source: &str) -> Vec<FixtureDiagnostic> {
    let checks = [
        ("filesystem", "std.io."),
        ("networking", "std.net."),
        ("processes", "std.process."),
        ("actors", "std.vm.Actor"),
        ("blocking-operations", "std.vm.Blocking"),
        ("native-boundary", "std.native."),
        ("heap", "std.collections."),
    ];
    source
        .lines()
        .enumerate()
        .flat_map(|(index, line)| {
            checks.iter().filter_map(move |(feature, marker)| {
                if line.contains(marker)
                    && feature_support(row, feature) == Some(SupportClass::Rejected)
                {
                    Some(FixtureDiagnostic {
                        code: DIAGNOSTIC_CODE,
                        target: row.target,
                        feature,
                        required_capability: required_capability(row, feature),
                        line: index + 1,
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn feature_support(row: &TargetRow, name: &str) -> Option<SupportClass> {
    row.features
        .iter()
        .find(|feature| feature.feature == name)
        .map(|feature| feature.support)
}

fn required_capability(row: &TargetRow, name: &str) -> Option<&'static str> {
    row.features
        .iter()
        .find(|feature| feature.feature == name)
        .and_then(|feature| feature.required_capability)
}

fn target<'a>(targets: &'a [TargetRow], name: &str) -> QualityResult<&'a TargetRow> {
    targets
        .iter()
        .find(|row| row.target == name)
        .ok_or_else(|| render_failure(&[format!("missing target row `{name}`")]))
}

fn write_report(
    report_path: &Path,
    targets: &[TargetRow],
    rejected_features: &BTreeSet<&str>,
    fixture_diagnostics: &[FixtureDiagnostic],
) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "{}: failed to create report directory: {error}",
                report_path.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.native-no-std-target-feasibility.v1",
        "support_claim": "feasibility-contract-only",
        "targets": targets,
        "rejected_features": rejected_features,
        "minimal_constrained_surface": [
            "pure functions",
            "fixed-size numeric types",
            "static data",
            "explicit memory policy",
            "explicit panic strategy",
            "no ambient runtime",
        ],
        "binding_policy": {
            "boundary": "generated Rust bindings or maintained embedded crates",
            "forbidden_core_ownership": [
                "device drivers",
                "cryptography",
                "protocol stacks",
                "allocator assumptions",
            ],
        },
        "fixture_diagnostics": fixture_diagnostics,
        "diagnostic_code": DIAGNOSTIC_CODE,
        "adversarial_cases": REQUIRED_ADVERSARIAL_CASES,
        "future_implementation_prerequisites": FUTURE_PREREQUISITES,
    });
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to serialize feasibility report: {error}"))?;
    fs::write(report_path, bytes).map_err(|error| {
        format!(
            "{}: failed to write feasibility report: {error}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    format!(
        "[native-no-std-target-feasibility] failed:\n{}",
        diagnostics
            .iter()
            .map(|diagnostic| format!("- {diagnostic}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[cfg(test)]
#[path = "native_no_std_target_feasibility_test.rs"]
#[cfg(test)]
mod native_no_std_target_feasibility_test;
