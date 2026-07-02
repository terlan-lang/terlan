use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{doctor_project, parse_doctor_args};

/// Creates an isolated doctor test directory.
fn doctor_test_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "terlan_doctor_{name}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create doctor test dir");
    path
}

/// Collects stable finding codes.
fn finding_codes(findings: &[super::DoctorFinding]) -> BTreeSet<&'static str> {
    findings.iter().map(|finding| finding.code).collect()
}

/// Verifies doctor argument parsing accepts the default project directory.
///
/// Inputs:
/// - Empty command-local argument vector.
///
/// Output:
/// - Parsed current-directory path.
///
/// Transformation:
/// - Locks the public command shape before filesystem diagnostics run.
#[test]
fn parse_doctor_args_defaults_to_current_directory() {
    assert_eq!(parse_doctor_args(&[]).expect("parse"), PathBuf::from("."));
}

/// Verifies doctor argument parsing rejects unsupported options.
///
/// Inputs:
/// - One unknown command-local flag.
///
/// Output:
/// - Stable usage error.
///
/// Transformation:
/// - Keeps doctor deterministic and project-dir only for the VM-pivot slice.
#[test]
fn parse_doctor_args_rejects_unknown_option() {
    assert_eq!(
        parse_doctor_args(&["--json".to_string()]),
        Err("unknown terlc doctor option: --json".to_string())
    );
}

/// Verifies a VM-shaped project produces no doctor findings.
///
/// Inputs:
/// - Temporary project manifest and one Terlan source file.
///
/// Output:
/// - Empty findings list.
///
/// Transformation:
/// - Scans the project using the same filesystem path as the command without
///   requiring any compiler build output.
#[test]
fn doctor_project_accepts_clean_vm_project() {
    let root = doctor_test_dir("clean_vm_project");
    fs::create_dir_all(root.join("src/app")).expect("create src dir");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("src/app/Main.terl"),
        "module app.Main.\n\npub main(): Unit ->\n    Unit.\n",
    )
    .expect("write source");

    let findings = doctor_project(&root).expect("doctor project");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(findings, Vec::new());
}

/// Verifies doctor reports VM-pivot migration hazards with exact codes.
///
/// Inputs:
/// - Temporary project containing retired manifest metadata, generated BEAM
///   output, `std.beam` imports, stale summaries, and a legacy Makefile.
///
/// Output:
/// - Findings covering each migration hazard.
///
/// Transformation:
/// - Runs the filesystem scanner end to end and asserts the user-facing code
///   set is stable.
#[test]
fn doctor_project_reports_vm_pivot_hazards() {
    let root = doctor_test_dir("vm_pivot_hazards");
    fs::create_dir_all(root.join("src/app")).expect("create src dir");
    fs::create_dir_all(root.join("_build/ebin")).expect("create ebin dir");
    fs::create_dir_all(root.join("summaries")).expect("create summaries dir");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"beam-thin\"\ntarget = \"erlang\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("src/app/Main.terl"),
        "module app.Main.\n\nimport std.beam.Agent.\n",
    )
    .expect("write source");
    fs::write(root.join("summaries/app.typi"), "summary\n").expect("write summary");
    fs::write(root.join("Makefile"), "test:\n\terlc src/app/Main.erl\n").expect("write makefile");

    let findings = doctor_project(&root).expect("doctor project");
    let codes = finding_codes(&findings);
    let _ = fs::remove_dir_all(&root);

    assert!(codes.contains("doctor_retired_manifest_artifact"));
    assert!(codes.contains("doctor_retired_runtime_target"));
    assert!(codes.contains("doctor_generated_beam_output"));
    assert!(codes.contains("doctor_retired_std_beam_import"));
    assert!(codes.contains("doctor_stale_summary_artifact"));
    assert!(codes.contains("doctor_retired_test_or_script_runtime"));
}

/// Verifies doctor reports checked source that current VM execution cannot run.
///
/// Inputs:
/// - Temporary project with a typechecked `case` expression.
///
/// Output:
/// - Finding set containing the VM execution-gap diagnostic.
///
/// Transformation:
/// - Runs the normal source scanner so the diagnostic is based on checked
///   CoreIR and the current VM artifact lowering subset.
#[test]
fn doctor_project_reports_vm_execution_gap_for_checked_coreir() {
    let root = doctor_test_dir("vm_execution_gap");
    fs::create_dir_all(root.join("src/app")).expect("create src dir");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("src/app/Main.terl"),
        "module app.Main.\n\npub classify(value: Int): Int ->\n    case value {\n        0 -> 1;\n        _ -> value\n    }.\n",
    )
    .expect("write source");

    let findings = doctor_project(&root).expect("doctor project");
    let codes = finding_codes(&findings);
    let _ = fs::remove_dir_all(&root);

    assert!(codes.contains("doctor_vm_execution_gap"));
}

/// Verifies doctor detects summaries generated against another compiler
/// syntax contract.
///
/// Inputs:
/// - Temporary project with a `.typi` summary and stale `.typi.deps`
///   fingerprint.
///
/// Output:
/// - Finding set containing the summary/compiler mismatch diagnostic.
///
/// Transformation:
/// - Scans generated summary metadata without rebuilding interfaces, matching
///   the migration doctor path used for application projects.
#[test]
fn doctor_project_reports_summary_compiler_contract_mismatch() {
    let root = doctor_test_dir("summary_contract_mismatch");
    fs::create_dir_all(root.join("summaries")).expect("create summaries dir");
    fs::write(root.join("summaries/app.typi"), "module app.\n").expect("write typi");
    fs::write(
        root.join("summaries/app.typi.deps"),
        "module=app\nsyntax_contract_fingerprint=fnv1a64:0000000000000000\n",
    )
    .expect("write deps");

    let findings = doctor_project(&root).expect("doctor project");
    let codes = finding_codes(&findings);
    let _ = fs::remove_dir_all(&root);

    assert!(codes.contains("doctor_summary_compiler_mismatch"));
}

/// Verifies Battleship-shaped projects receive exact VM migration wording.
///
/// Inputs:
/// - Temporary manifest with package name `battleship` and retired
///   `beam-thin` artifact metadata.
///
/// Output:
/// - Manifest-artifact finding whose fix names `terlc clean`, `terlc doctor`,
///   and `terlc build` in order.
///
/// Transformation:
/// - Runs manifest scanning through doctor so application-specific migration
///   text stays covered by the public command gate.
#[test]
fn doctor_project_reports_battleship_manifest_migration_fix() {
    let root = doctor_test_dir("battleship_manifest_fix");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"battleship\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"beam-thin\"\n",
    )
    .expect("write manifest");

    let findings = doctor_project(&root).expect("doctor project");
    let finding = findings
        .iter()
        .find(|finding| finding.code == "doctor_retired_manifest_artifact")
        .expect("manifest finding");
    let _ = fs::remove_dir_all(&root);

    assert!(finding.fix.contains("artifact = \"terlan-vm\""));
    assert!(finding.fix.contains("terlc clean"));
    assert!(finding.fix.contains("terlc doctor"));
    assert!(finding.fix.contains("terlc build"));
}
