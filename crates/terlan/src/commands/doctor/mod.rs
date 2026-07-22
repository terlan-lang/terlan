use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::terlan_syntax::cached_canonical_terlan_syntax_contract_identity;
use crate::CliCommand;

const MANIFEST_FILE: &str = "terlan.toml";
const GENERATED_OUTPUTS: &[&str] = &["_build/src", "_build/ebin"];
const SOURCE_EXTENSIONS: &[&str] = &["terl", "terli"];
const SCRIPT_EXTENSIONS: &[&str] = &["sh", "mk"];
const SKIPPED_DIRS: &[&str] = &[".git", "target", "node_modules"];

/// VM-pivot diagnostic found by `terlc doctor`.
///
/// Inputs:
/// - `path`: project-relative file or directory containing the finding.
/// - `code`: stable diagnostic code.
/// - `message`: human-readable problem summary.
/// - `fix`: exact suggested migration step.
///
/// Output:
/// - Renderable diagnostic used by CLI output and tests.
///
/// Transformation:
/// - Keeps diagnosis and suggested fix together so `terlc doctor` is
///   actionable instead of becoming another passive inventory command.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorFinding {
    path: PathBuf,
    code: &'static str,
    message: String,
    fix: String,
}

impl DoctorFinding {
    /// Renders one finding in a stable text format.
    fn render(&self) -> String {
        format!(
            "{}: {}: {}\n  fix: {}",
            self.path.display(),
            self.code,
            self.message,
            self.fix
        )
    }
}

/// Executes the `doctor` CLI command.
///
/// Inputs:
/// - `cmd`: parsed command-local args; accepts zero args or one project dir.
///
/// Output:
/// - Success when no VM-pivot findings are present.
/// - Exit 1 when project migration findings are reported.
/// - Exit 2 for malformed command-local arguments.
///
/// Transformation:
/// - Scans project manifests, generated outputs, source imports, summary
///   artifacts, and script/test command text for removed OTP/BEAM-era
///   constructs, then prints exact VM-pivot fixes.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    let project_dir = match parse_doctor_args(&cmd.args) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            crate::print_command_usage("doctor");
            return ExitCode::from(2);
        }
    };

    match doctor_project(&project_dir) {
        Ok(findings) if findings.is_empty() => {
            println!("terlc doctor: ok");
            ExitCode::SUCCESS
        }
        Ok(findings) => {
            for finding in findings {
                println!("{}", finding.render());
            }
            ExitCode::from(1)
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Parses command-local `doctor` args.
fn parse_doctor_args(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Ok(PathBuf::from(".")),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => {
            Err("terlc doctor accepts one optional project directory".to_string())
        }
        [path] if !path.starts_with('-') => Ok(PathBuf::from(path)),
        [flag] if flag.starts_with('-') => Err(format!("unknown terlc doctor option: {flag}")),
        _ => Err("terlc doctor accepts at most one project directory".to_string()),
    }
}

/// Runs VM-pivot diagnostics for one project directory.
fn doctor_project(project_dir: &Path) -> Result<Vec<DoctorFinding>, String> {
    let root = project_dir.canonicalize().map_err(|err| {
        format!(
            "cannot open project directory {}: {err}",
            project_dir.display()
        )
    })?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", project_dir.display()));
    }

    let mut findings = Vec::new();
    scan_manifest(&root, &mut findings)?;
    scan_generated_outputs(&root, &mut findings);
    for path in project_files(&root)? {
        scan_source_file(&root, &path, &mut findings)?;
        scan_summary_file(&root, &path, &mut findings);
        scan_script_file(&root, &path, &mut findings)?;
    }
    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.code.cmp(right.code))
            .then_with(|| left.message.cmp(&right.message))
    });
    Ok(findings)
}

/// Scans the project manifest for retired artifact/runtime metadata.
fn scan_manifest(root: &Path, findings: &mut Vec<DoctorFinding>) -> Result<(), String> {
    let path = root.join(MANIFEST_FILE);
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let relative = relative_path(root, &path);
    if text.contains("artifact = \"beam-thin\"") {
        findings.push(DoctorFinding {
            path: relative.clone(),
            code: "doctor_retired_manifest_artifact",
            message: "manifest uses retired BEAM artifact metadata".to_string(),
            fix: retired_manifest_artifact_fix(&root, &text),
        });
    }
    if text.contains("target = \"erlang\"") || text.contains("runtime = \"beam\"") {
        findings.push(DoctorFinding {
            path: relative,
            code: "doctor_retired_runtime_target",
            message: "manifest references removed Erlang/BEAM runtime selection".to_string(),
            fix: "remove the runtime override and let Terlan use the default VM target".to_string(),
        });
    }
    Ok(())
}

/// Builds an exact fix for retired manifest artifact metadata.
fn retired_manifest_artifact_fix(root: &Path, _manifest_text: &str) -> String {
    let project = root.display();
    format!(
        "edit terlan.toml: replace `artifact = \"beam-thin\"` with `artifact = \"terlan-vm\"`; run `terlc clean {project}` and `terlc build {project}`"
    )
}

/// Scans generated output directories that should be removed during migration.
fn scan_generated_outputs(root: &Path, findings: &mut Vec<DoctorFinding>) {
    for relative in GENERATED_OUTPUTS {
        let path = root.join(relative);
        if path.exists() {
            findings.push(DoctorFinding {
                path: PathBuf::from(relative),
                code: "doctor_generated_beam_output",
                message: "project contains generated Erlang/BEAM output".to_string(),
                fix: "run `terlc clean` and rebuild with the VM target".to_string(),
            });
        }
    }
}

/// Returns project files that doctor should scan.
fn project_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_project_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collects files while skipping generated dependency roots.
fn collect_project_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("cannot read directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("cannot read directory entry: {err}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_project_files(root, &path, files)?;
        } else if path.is_file() && path != root.join(MANIFEST_FILE) {
            files.push(path);
        }
    }
    Ok(())
}

/// Scans Terlan source files for retired VM-facing imports.
fn scan_source_file(
    root: &Path,
    path: &Path,
    findings: &mut Vec<DoctorFinding>,
) -> Result<(), String> {
    if !has_extension(path, SOURCE_EXTENSIONS) {
        return Ok(());
    }
    let text =
        fs::read_to_string(path).map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    if text.contains("std.beam") {
        findings.push(DoctorFinding {
            path: relative_path(root, path),
            code: "doctor_retired_std_beam_import",
            message: "source imports retired `std.beam` modules".to_string(),
            fix: "replace `std.beam.*` imports with the matching `std.vm.*` module".to_string(),
        });
    }
    Ok(())
}

/// Scans generated summary artifacts that should not survive app migration.
fn scan_summary_file(root: &Path, path: &Path, findings: &mut Vec<DoctorFinding>) {
    let is_typi = path.extension().and_then(|ext| ext.to_str()) == Some("typi");
    let is_summary_artifact = is_typi
        || relative_path(root, path)
            .components()
            .any(|component| component.as_os_str() == "summaries");
    if is_summary_artifact {
        findings.push(DoctorFinding {
            path: relative_path(root, path),
            code: "doctor_stale_summary_artifact",
            message: "project contains generated summary artifacts".to_string(),
            fix: "delete generated summaries and let the installed compiler regenerate interfaces"
                .to_string(),
        });
    }
    if is_typi {
        scan_summary_compiler_contract(root, path, findings);
    }
}

/// Scans `.typi` dependency metadata against the installed compiler contract.
fn scan_summary_compiler_contract(root: &Path, path: &Path, findings: &mut Vec<DoctorFinding>) {
    let deps_path = summary_dependency_path(path);
    if !deps_path.exists() {
        findings.push(DoctorFinding {
            path: relative_path(root, path),
            code: "doctor_summary_missing_compiler_metadata",
            message: "summary cannot be matched to the installed compiler".to_string(),
            fix: "delete the `.typi` file or regenerate summaries with the installed compiler"
                .to_string(),
        });
        return;
    }

    let Ok(deps_text) = fs::read_to_string(&deps_path) else {
        findings.push(DoctorFinding {
            path: relative_path(root, &deps_path),
            code: "doctor_summary_unreadable_compiler_metadata",
            message: "summary compiler metadata could not be read".to_string(),
            fix: "delete the `.typi.deps` file or regenerate summaries with the installed compiler"
                .to_string(),
        });
        return;
    };
    let Some(found) = summary_contract_fingerprint(&deps_text) else {
        findings.push(DoctorFinding {
            path: relative_path(root, &deps_path),
            code: "doctor_summary_missing_compiler_metadata",
            message: "summary metadata does not include a syntax-contract fingerprint".to_string(),
            fix: "regenerate summaries with the installed compiler".to_string(),
        });
        return;
    };
    let Ok(current) = cached_canonical_terlan_syntax_contract_identity() else {
        return;
    };
    if found != current.fingerprint {
        findings.push(DoctorFinding {
            path: relative_path(root, &deps_path),
            code: "doctor_summary_compiler_mismatch",
            message: format!(
                "summary was generated for syntax contract `{found}`, but installed compiler expects `{}`",
                current.fingerprint
            ),
            fix: "delete generated summaries and regenerate them with the installed compiler"
                .to_string(),
        });
    }
}

/// Returns the sidecar metadata path for one `.typi` summary.
fn summary_dependency_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.deps", path.display()))
}

/// Extracts a syntax-contract fingerprint from summary dependency metadata.
fn summary_contract_fingerprint(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("syntax_contract_fingerprint="))
        .map(str::to_string)
}

/// Scans script and Make files for removed OTP-era command paths.
fn scan_script_file(
    root: &Path,
    path: &Path,
    findings: &mut Vec<DoctorFinding>,
) -> Result<(), String> {
    let is_makefile = path.file_name().and_then(|name| name.to_str()) == Some("Makefile");
    if !is_makefile && !has_extension(path, SCRIPT_EXTENSIONS) {
        return Ok(());
    }
    let text =
        fs::read_to_string(path).map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let has_removed_command = text.contains("erlc")
        || text.contains(" eunit")
        || text.contains("--target erlang")
        || text.contains("--runtime beam")
        || text.contains("beam-thin")
        || text.contains("_build/ebin");
    if has_removed_command {
        findings.push(DoctorFinding {
            path: relative_path(root, path),
            code: "doctor_retired_test_or_script_runtime",
            message: "script or test command references removed OTP/BEAM paths".to_string(),
            fix: "replace legacy commands with `terlc build`, `terlc test`, `terlc run`, or `terlc clean`"
                .to_string(),
        });
    }
    Ok(())
}

/// Returns whether a path has one of the given extensions.
fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.contains(&ext))
}

/// Returns a project-relative path for stable diagnostics.
fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

#[cfg(test)]
#[path = "doctor_test.rs"]
mod doctor_test;
