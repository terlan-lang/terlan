use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::terlan_quality::QualityResult;

const OUTPUT_DIR: &str = "_build/erlang-modernization";

const REQUIRED_ARTIFACTS: &[&str] = &[
    "reference-otp-baseline",
    "terlan-reference-smokes",
    "runtime-surface",
    "test-inventory",
    "compatibility-probes",
    "strip-candidates",
    "replacement-candidates",
    "reduced-otp-profile",
];

const REQUIRED_CORE_APPS: &[&str] = &[
    "common_test",
    "compiler",
    "kernel",
    "parsetools",
    "sasl",
    "stdlib",
    "syntax_tools",
    "tools",
];

const REMOVED_APP_BASELINE: &[&str] = &[
    "wx",
    "megaco",
    "reltool",
    "inets",
    "debugger",
    "observer",
    "et",
    "ftp",
    "tftp",
    "eldap",
    "diameter",
    "snmp",
    "odbc",
    "jinterface",
    "xmerl",
    "edoc",
    "os_mon",
    "ssh",
    "erl_interface",
    "dialyzer",
];

/// Summary produced by the Erlang modernization EM0 inventory gate.
///
/// Inputs:
/// - Repository root and sibling reduced OTP compatibility runtime source tree.
///
/// Output:
/// - Counts for the reduced OTP app inventory and emitted artifacts.
///
/// Transformation:
/// - Provides stable CLI output for the internal quality command while the
///   detailed evidence is written to JSON/Markdown artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErlangModernizationSummary {
    pub artifact_count: usize,
    pub kept_app_count: usize,
    pub removed_app_count: usize,
}

/// Summary produced by the Erlang runtime matrix gate.
///
/// Inputs:
/// - OTP 29 runtime path, local OTP compatibility runtime path, and a shared
///   test command.
///
/// Output:
/// - Runtime lane count and executed command text.
///
/// Transformation:
/// - Gives CI and local runs a stable success summary after both runtimes have
///   proven they can run the same Terlan test command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErlangRuntimeMatrixSummary {
    pub lane_count: usize,
    pub command: String,
}

/// Runs the EM0 Erlang modernization inventory gate.
///
/// Inputs:
/// - `root`: golden repository root.
///
/// Output:
/// - `ErlangModernizationSummary` when the reference tree is present, core apps
///   are available, removed-app baseline entries are absent, and artifacts are
///   emitted.
/// - Error string when evidence is missing.
///
/// Transformation:
/// - Reads the sibling reduced OTP compatibility checkout, validates the
///   reduced OTP profile, and emits deterministic JSON/Markdown evidence under
///   `_build/erlang-modernization`.
pub fn run_erlang_modernization_inventory(
    root: &Path,
) -> QualityResult<ErlangModernizationSummary> {
    let root = root.canonicalize().map_err(|err| {
        format!(
            "erlang-modernization-inventory-check cannot canonicalize {}: {err}",
            root.display()
        )
    })?;
    let vm_root = root
        .parent()
        .ok_or_else(|| "erlang-modernization-inventory-check cannot resolve parent".to_string())?
        .join("terlan-vm");
    let lib_dir = vm_root.join("lib");
    if !lib_dir.is_dir() {
        return Err(format!(
            "erlang-modernization-inventory-check missing reference VM lib directory: {}",
            lib_dir.display()
        ));
    }
    assert_no_experimental_runtime_dependency(&root)?;

    let apps = otp_applications(&lib_dir)?;
    let missing_core = missing_entries(REQUIRED_CORE_APPS, &apps);
    if !missing_core.is_empty() {
        return Err(format!(
            "erlang-modernization-inventory-check missing required OTP apps: {}",
            missing_core.join(", ")
        ));
    }

    let removed_present = present_entries(REMOVED_APP_BASELINE, &apps);
    if !removed_present.is_empty() {
        return Err(format!(
            "erlang-modernization-inventory-check reduced VM still includes removed apps: {}",
            removed_present.join(", ")
        ));
    }

    let output_dir = root.join(OUTPUT_DIR);
    fs::create_dir_all(&output_dir).map_err(|err| {
        format!(
            "erlang-modernization-inventory-check cannot create {}: {err}",
            output_dir.display()
        )
    })?;

    let artifacts = artifacts(&root, &vm_root, &apps);
    for (name, value) in artifacts {
        write_artifact_pair(&output_dir, name, &value)?;
    }
    ensure_required_artifacts(&output_dir)?;

    Ok(ErlangModernizationSummary {
        artifact_count: REQUIRED_ARTIFACTS.len(),
        kept_app_count: apps.len(),
        removed_app_count: REMOVED_APP_BASELINE.len(),
    })
}

/// Runs the Terlan test suite command against OTP 29 and the local compatibility runtime.
///
/// Inputs:
/// - `root`: golden repository root.
/// - Environment:
///   - `TERLAN_OTP29_BIN`: required directory containing OTP 29 `erl` and
///     `erlc`.
///   - `TERLAN_OTP_RUNTIME_BIN`: required directory containing the local OTP
///     compatibility runtime `erl` and `erlc`.
///   - `TERLAN_RUNTIME_MATRIX_COMMAND`: optional shell command run under both
///     runtimes; defaults to `make cli-test-fast`.
///
/// Output:
/// - `ErlangRuntimeMatrixSummary` when both runtime lanes report the expected
///   identity and the shared command succeeds under each lane.
/// - Error string identifying the first failed runtime or command.
///
/// Transformation:
/// - Prepends the selected runtime `bin` directory to `PATH` and executes the
///   same command for each lane. This validates Terlan behavior against both
///   the reference OTP 29 lane and the local OTP compatibility runtime lane
///   without changing compiler source paths.
pub fn run_erlang_runtime_matrix(root: &Path) -> QualityResult<ErlangRuntimeMatrixSummary> {
    let root = root.canonicalize().map_err(|err| {
        format!(
            "erlang-runtime-matrix-check cannot canonicalize {}: {err}",
            root.display()
        )
    })?;
    let command =
        env::var("TERLAN_RUNTIME_MATRIX_COMMAND").unwrap_or_else(|_| "make cli-test-fast".into());
    let otp29_bin = env::var("TERLAN_OTP29_BIN")
        .map(PathBuf::from)
        .map_err(|_| {
            "erlang-runtime-matrix-check requires TERLAN_OTP29_BIN pointing to OTP 29 bin"
                .to_string()
        })?;
    let otp_runtime_bin = env::var("TERLAN_OTP_RUNTIME_BIN")
        .map(PathBuf::from)
        .map_err(|_| {
            "erlang-runtime-matrix-check requires TERLAN_OTP_RUNTIME_BIN pointing to the local OTP compatibility runtime bin"
                .to_string()
        });
    let otp_runtime_bin = otp_runtime_bin?;

    let lanes = [
        RuntimeLane {
            name: "otp29",
            bin_dir: otp29_bin,
            expected_otp_release: Some("29"),
        },
        RuntimeLane {
            name: "otp-runtime",
            bin_dir: otp_runtime_bin,
            expected_otp_release: None,
        },
    ];

    for lane in lanes {
        run_runtime_lane(&root, &lane, &command)?;
    }

    Ok(ErlangRuntimeMatrixSummary {
        lane_count: 2,
        command,
    })
}

/// One Erlang runtime lane exercised by the runtime matrix gate.
struct RuntimeLane {
    name: &'static str,
    bin_dir: PathBuf,
    expected_otp_release: Option<&'static str>,
}

/// Runs the selected Terlan command under one Erlang runtime lane.
fn run_runtime_lane(root: &Path, lane: &RuntimeLane, command: &str) -> QualityResult<()> {
    ensure_runtime_binary(lane, "erl")?;
    ensure_runtime_binary(lane, "erlc")?;
    let release = runtime_otp_release(lane)?;
    if let Some(expected) = lane.expected_otp_release {
        if release != expected {
            return Err(format!(
                "erlang-runtime-matrix-check lane `{}` expected OTP release {expected}, got {release} from {}",
                lane.name,
                lane.bin_dir.display()
            ));
        }
    }
    let old_path = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![lane.bin_dir.clone()];
    paths.extend(env::split_paths(&old_path));
    let path = env::join_paths(paths).map_err(|err| {
        format!(
            "erlang-runtime-matrix-check lane `{}` cannot build PATH: {err}",
            lane.name
        )
    })?;
    let status = Command::new("bash")
        .arg("-lc")
        .arg(command)
        .current_dir(root)
        .env("PATH", path)
        .env("TERLAN_RUNTIME_MATRIX_LANE", lane.name)
        .status()
        .map_err(|err| {
            format!(
                "erlang-runtime-matrix-check lane `{}` failed to start `{command}`: {err}",
                lane.name
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "erlang-runtime-matrix-check lane `{}` command `{command}` failed with status {status}",
            lane.name
        ))
    }
}

/// Verifies that a runtime lane exposes one required binary.
fn ensure_runtime_binary(lane: &RuntimeLane, name: &str) -> QualityResult<()> {
    let path = lane.bin_dir.join(name);
    if path.is_file() {
        Ok(())
    } else {
        Err(format!(
            "erlang-runtime-matrix-check lane `{}` missing {}",
            lane.name,
            path.display()
        ))
    }
}

/// Reads the OTP release reported by a runtime lane.
fn runtime_otp_release(lane: &RuntimeLane) -> QualityResult<String> {
    let output = Command::new(lane.bin_dir.join("erl"))
        .arg("-noshell")
        .arg("-eval")
        .arg("io:format(\"~s~n\", [erlang:system_info(otp_release)]), halt().")
        .output()
        .map_err(|err| {
            format!(
                "erlang-runtime-matrix-check lane `{}` cannot run erl: {err}",
                lane.name
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "erlang-runtime-matrix-check lane `{}` erl version probe failed with status {}",
            lane.name, output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Lists OTP applications present under an OTP `lib` directory.
fn otp_applications(lib_dir: &Path) -> QualityResult<BTreeSet<String>> {
    let mut apps = BTreeSet::new();
    for entry in fs::read_dir(lib_dir).map_err(|err| {
        format!(
            "erlang-modernization-inventory-check cannot read {}: {err}",
            lib_dir.display()
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "erlang-modernization-inventory-check cannot read entry in {}: {err}",
                lib_dir.display()
            )
        })?;
        if entry.path().is_dir() {
            apps.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(apps)
}

/// Returns expected inventory entries that are absent from an actual set.
fn missing_entries(expected: &[&str], actual: &BTreeSet<String>) -> Vec<String> {
    expected
        .iter()
        .filter(|entry| !actual.contains::<str>(*entry))
        .map(|entry| (*entry).to_string())
        .collect()
}

/// Returns entries that should be absent but are present in an actual set.
fn present_entries(expected_absent: &[&str], actual: &BTreeSet<String>) -> Vec<String> {
    expected_absent
        .iter()
        .filter(|entry| actual.contains::<str>(*entry))
        .map(|entry| (*entry).to_string())
        .collect()
}

/// Verifies release manifests do not depend on the experimental runtime.
fn assert_no_experimental_runtime_dependency(root: &Path) -> QualityResult<()> {
    let manifests = [
        root.join("Cargo.toml"),
        root.join("crates/terlan/Cargo.toml"),
        root.join("Cargo.lock"),
    ];
    let forbidden = ["terlan-vm", "experimental-runtime", "reduced-otp-runtime"];
    let mut diagnostics = Vec::new();
    for manifest in manifests {
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        for marker in forbidden {
            if text.contains(marker) {
                diagnostics.push(format!("{} contains `{marker}`", manifest.display()));
            }
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "erlang-modernization-inventory-check found experimental runtime dependency: {}",
            diagnostics.join(", ")
        ))
    }
}

/// Builds all Erlang modernization inventory artifacts.
fn artifacts(root: &Path, vm_root: &Path, apps: &BTreeSet<String>) -> Vec<(&'static str, Value)> {
    vec![
        (
            "reference-otp-baseline",
            reference_otp_baseline(vm_root, apps),
        ),
        ("terlan-reference-smokes", terlan_reference_smokes()),
        ("runtime-surface", runtime_surface(root)),
        ("test-inventory", test_inventory(root)),
        ("compatibility-probes", compatibility_probes()),
        ("strip-candidates", strip_candidates(apps)),
        ("replacement-candidates", replacement_candidates()),
        ("reduced-otp-profile", reduced_otp_profile(apps)),
    ]
}

/// Builds the reference OTP baseline inventory artifact.
fn reference_otp_baseline(vm_root: &Path, apps: &BTreeSet<String>) -> Value {
    json!({
        "schema": "terlan-erlang-modernization-reference-otp-baseline-v1",
        "reference_vm": relative_display(vm_root),
        "otp_versions_table": vm_root.join("otp_versions.table").is_file(),
        "readme": vm_root.join("README.md").is_file(),
        "configure_present": vm_root.join("configure").is_file(),
        "experimental_runtime_dependency": false,
        "full_test_command": [
            "cd terlan-vm",
            "export ERL_TOP=$(pwd)",
            "./configure && make",
            "make test"
        ],
        "applications": apps.iter().cloned().collect::<Vec<_>>(),
        "required_core_applications": REQUIRED_CORE_APPS,
        "removed_applications_absent": REMOVED_APP_BASELINE,
    })
}

/// Builds the Terlan smoke-test inventory artifact.
fn terlan_reference_smokes() -> Value {
    json!({
        "schema": "terlan-erlang-modernization-reference-smokes-v1",
        "reference_runtime": "regular OTP/ERTS",
        "smokes": [
            {
                "name": "cli fast tests",
                "command": "make cli-test-fast",
                "classification": "runnable_now"
            },
            {
                "name": "stdlib release check",
                "command": "make stdlib-release-check",
                "classification": "runnable_now"
            },
            {
                "name": "release-scale tests",
                "command": "make test-release",
                "classification": "runnable_now"
            }
        ]
    })
}

/// Builds the generated Erlang runtime-surface inventory artifact.
fn runtime_surface(root: &Path) -> Value {
    json!({
        "schema": "terlan-erlang-modernization-runtime-surface-v1",
        "generated_erlang_surface": {
            "source_roots": [
                "crates/terlan/src/backends/erlang",
                "crates/terlan/src/commands/build"
            ],
            "known_runtime_modules": [
                "erlang",
                "io",
                "lists",
                "maps",
                "binary",
                "code",
                "erl_eval",
                "init"
            ],
            "safe_native_boundary": "crates/terlan/src/runtime/safenative"
        },
        "source_files_scanned": count_files(root, &["crates/terlan/src/backends/erlang"], "rs")
    })
}

/// Builds the runtime modernization test-inventory artifact.
fn test_inventory(root: &Path) -> Value {
    json!({
        "schema": "terlan-erlang-modernization-test-inventory-v1",
        "classes": [
            {
                "name": "terlan exact cargo gates",
                "classification": "runnable_now",
                "owner": "Makefile and crates/terlan/cli.mk"
            },
            {
                "name": "OTP full test suite",
                "classification": "runnable_with_harness_work",
                "owner": "terlan-vm"
            },
            {
                "name": "runtime failure-injection probes",
                "classification": "missing",
                "owner": "EM0.6"
            }
        ],
        "terlan_fixture_count": count_files(root, &["tests", "std"], "terl")
    })
}

/// Builds the compatibility-probe inventory artifact.
fn compatibility_probes() -> Value {
    json!({
        "schema": "terlan-erlang-modernization-compatibility-probes-v1",
        "probes": [
            "spawn_link_monitor_exit",
            "message_ordering",
            "selective_receive",
            "timers_and_cancellation",
            "atom_limits",
            "binary_lifetime",
            "term_roundtrip",
            "heap_observable_behavior",
            "crash_and_supervision"
        ],
        "status": "defined_not_yet_executed"
    })
}

/// Builds the OTP application strip-candidate artifact.
fn strip_candidates(apps: &BTreeSet<String>) -> Value {
    let candidates = REMOVED_APP_BASELINE
        .iter()
        .map(|app| {
            json!({
                "application": app,
                "decision": "strip",
                "present_in_reduced_tree": apps.contains::<str>(*app),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "terlan-erlang-modernization-strip-candidates-v1",
        "candidates": candidates
    })
}

/// Builds the replacement-candidate inventory artifact.
fn replacement_candidates() -> Value {
    json!({
        "schema": "terlan-erlang-modernization-replacement-candidates-v1",
        "first_candidates": [
            "runtime capability metadata",
            "heap sizing policy",
            "statistics/accounting helpers",
            "portable parsing/formatting helpers"
        ],
        "constraint": "no ERTS replacement in EM0"
    })
}

/// Builds the reduced OTP profile inventory artifact.
fn reduced_otp_profile(apps: &BTreeSet<String>) -> Value {
    json!({
        "schema": "terlan-erlang-modernization-reduced-otp-profile-v1",
        "kept_applications": apps.iter().cloned().collect::<Vec<_>>(),
        "removed_applications": REMOVED_APP_BASELINE,
        "decision": "reduced tree is evidence inventory only; regular OTP/ERTS remains reference runtime"
    })
}

/// Counts files with one extension below several repository-relative roots.
fn count_files(root: &Path, directories: &[&str], extension: &str) -> usize {
    directories
        .iter()
        .map(|directory| count_files_in_dir(&root.join(directory), extension))
        .sum()
}

/// Recursively counts files with one extension under one directory.
fn count_files_in_dir(directory: &Path, extension: &str) -> usize {
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .map(|path| {
            if path.is_dir() {
                count_files_in_dir(&path, extension)
            } else if path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
                1
            } else {
                0
            }
        })
        .sum()
}

/// Writes matching JSON and Markdown artifacts for one inventory value.
fn write_artifact_pair(output_dir: &Path, name: &str, value: &Value) -> QualityResult<()> {
    let json_path = output_dir.join(format!("{name}.json"));
    let json_text = serde_json::to_string_pretty(value)
        .map_err(|err| format!("cannot serialize {name}.json: {err}"))?;
    fs::write(&json_path, format!("{json_text}\n")).map_err(|err| {
        format!(
            "erlang-modernization-inventory-check cannot write {}: {err}",
            json_path.display()
        )
    })?;

    let md_path = output_dir.join(format!("{name}.md"));
    fs::write(&md_path, markdown_artifact(name, value)).map_err(|err| {
        format!(
            "erlang-modernization-inventory-check cannot write {}: {err}",
            md_path.display()
        )
    })?;
    Ok(())
}

/// Renders one inventory value as a compact Markdown artifact.
fn markdown_artifact(name: &str, value: &Value) -> String {
    let title = name
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let json_text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    format!("# {title}\n\n```json\n{json_text}\n```\n")
}

/// Verifies that every required inventory artifact was emitted.
fn ensure_required_artifacts(output_dir: &Path) -> QualityResult<()> {
    let mut missing = Vec::new();
    for artifact in REQUIRED_ARTIFACTS {
        for extension in ["json", "md"] {
            let path = output_dir.join(format!("{artifact}.{extension}"));
            if !path.is_file() {
                missing.push(relative_display(&path));
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "erlang-modernization-inventory-check missing emitted artifacts: {}",
            missing.join(", ")
        ))
    }
}

/// Formats a path for stable inventory diagnostics.
fn relative_display(path: &Path) -> String {
    path.components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .into_owned()
}
