use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::artifacts::atomic_write;
use super::oracle;
use crate::{GeneratedCase, GeneratedSource, Oracle};

const ENTRY_SOURCE: &str = "CaseTest.terl";
const PROJECT_MANIFEST: &str = "terlan.toml";
const ROUNDTRIP_ARTIFACT: &str = "interface-roundtrip.json";
const SOURCE_ROOT: &str = "src";

/// Rechecks a generated consumer against only its emitted interfaces.
///
/// The source-backed oracle has already accepted the same consumer. This
/// isolated workspace deliberately contains no provider implementation files,
/// so acceptance proves that the generated `.typi` summaries preserve every
/// type fact exercised by the consumer.
pub(super) fn run(
    directory: &Path,
    case: &GeneratedCase,
    compiler: &Path,
    timeout_ms: u64,
    rss_limit_bytes: u64,
    cpu_limit_ms: u64,
) -> Result<(), String> {
    if case.support_sources.is_empty() || case.manifest.oracle == Oracle::None {
        return Ok(());
    }
    let modules = support_modules(&case.support_sources)?;
    let workspace = tempfile::Builder::new()
        .prefix(".interface-roundtrip-")
        .tempdir_in(directory)
        .map_err(|error| format!("create interface round-trip workspace: {error}"))?;
    let interface_cache = directory.join(SOURCE_ROOT).join(".terlan");
    let result = if interfaces_complete(&interface_cache, &modules) {
        stage(workspace.path(), directory, &interface_cache, &modules)?;
        oracle::run(
            compiler,
            Oracle::Check,
            &workspace.path().join(ENTRY_SOURCE),
            timeout_ms,
            rss_limit_bytes,
            cpu_limit_ms,
        )
    } else {
        let generation = oracle::run(
            compiler,
            Oracle::Check,
            &directory.join(SOURCE_ROOT),
            timeout_ms,
            rss_limit_bytes,
            cpu_limit_ms,
        );
        if generation.outcome != "passed" {
            record_evidence(case, directory, &modules, "not_run", &generation)?;
            return Err(format!(
                "case {} interface generation ended as {}",
                case.manifest.case_id, generation.outcome
            ));
        }
        stage(workspace.path(), directory, &interface_cache, &modules)?;
        oracle::finish_phase(
            generation,
            timeout_ms,
            rss_limit_bytes,
            cpu_limit_ms,
            |remaining_wall, remaining_cpu| {
                oracle::run(
                    compiler,
                    Oracle::Check,
                    &workspace.path().join(ENTRY_SOURCE),
                    remaining_wall,
                    rss_limit_bytes,
                    remaining_cpu,
                )
            },
        )
    };
    record_evidence(case, directory, &modules, result.outcome, &result)?;
    if result.outcome != "passed" {
        return Err(format!(
            "case {} interface-only semantic comparison ended as {}",
            case.manifest.case_id, result.outcome
        ));
    }
    Ok(())
}

fn interfaces_complete(cache: &Path, modules: &[String]) -> bool {
    modules
        .iter()
        .all(|module| cache.join(format!("{module}.typi")).is_file())
}

fn record_evidence(
    case: &GeneratedCase,
    directory: &Path,
    modules: &[String],
    interface_outcome: &str,
    result: &oracle::OracleResult,
) -> Result<(), String> {
    let evidence = serde_json::json!({
        "schema": "terlan.type-fuzz-interface-roundtrip.v1",
        "comparison": "source_and_interface_consumers_accepted",
        "source_oracle": oracle_name(case.manifest.oracle),
        "source_outcome": "passed",
        "interface_oracle": "check",
        "interface_outcome": interface_outcome,
        "expected_type": case.manifest.expected_type,
        "modules": modules,
        "exit_code": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "resources": result.resources,
    });
    let mut bytes = serde_json::to_vec(&evidence).expect("interface evidence JSON serializes");
    bytes.push(b'\n');
    atomic_write(&directory.join(ROUNDTRIP_ARTIFACT), &bytes)
}

fn stage(
    workspace: &Path,
    directory: &Path,
    interface_cache: &Path,
    modules: &[String],
) -> Result<(), String> {
    let cache = workspace.join(".terlan");
    fs::create_dir_all(&cache)
        .map_err(|error| format!("create interface round-trip cache: {error}"))?;
    let source_root = workspace.join("src");
    fs::create_dir(&source_root)
        .map_err(|error| format!("create interface round-trip source root: {error}"))?;
    copy(directory, workspace, PROJECT_MANIFEST)?;
    copy(directory, workspace, ENTRY_SOURCE)?;
    for module in modules {
        let name = format!("{module}.typi");
        copy(interface_cache, &cache, &name)?;
    }
    Ok(())
}

pub(super) fn support_modules(sources: &[GeneratedSource]) -> Result<Vec<String>, String> {
    let mut modules = BTreeSet::new();
    for source in sources {
        let module = source_module(source)?;
        if !modules.insert(module.clone()) {
            return Err(format!("duplicate generated support module `{module}`"));
        }
    }
    Ok(modules.into_iter().collect())
}

fn source_module(source: &GeneratedSource) -> Result<String, String> {
    let module = source
        .source
        .lines()
        .find_map(|line| line.trim().strip_prefix("module "))
        .and_then(|line| line.strip_suffix('.'))
        .filter(|module| !module.is_empty() && module.split('.').all(valid_module_segment))
        .ok_or_else(|| {
            format!(
                "generated support source `{}` has no valid module declaration",
                source.path.display()
            )
        })?;
    Ok(module.to_string())
}

fn valid_module_segment(segment: &str) -> bool {
    let mut characters = segment.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn copy(from: &Path, to: &Path, name: &str) -> Result<(), String> {
    fs::copy(from.join(name), to.join(name))
        .map(|_| ())
        .map_err(|error| format!("copy interface round-trip input `{name}`: {error}"))
}

fn oracle_name(oracle: Oracle) -> &'static str {
    match oracle {
        Oracle::Check => "check",
        Oracle::JavaScript => "js",
        Oracle::Native => "native",
        Oracle::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::{GenerationConfig, RunOptions};

    fn source(path: &str, text: &str) -> GeneratedSource {
        GeneratedSource {
            path: PathBuf::from(path),
            source: text.into(),
        }
    }

    fn generated_case(oracle: Oracle) -> GeneratedCase {
        let options = RunOptions::default();
        crate::generate(
            17,
            &GenerationConfig {
                seed: 1,
                max_depth: 8,
                oracle,
                timeout_ms: options.case_timeout_ms,
                rss_limit_bytes: options.rss_limit_bytes,
                cpu_limit_ms: options.cpu_limit_ms,
                compiler_revision: "test",
            },
        )
    }

    fn prepared_case(oracle: Oracle) -> (tempfile::TempDir, GeneratedCase) {
        let root = tempfile::tempdir().expect("interface case root");
        let case = generated_case(oracle);
        super::super::artifacts::write_case(root.path(), &case).expect("case artifacts");
        let cache = root.path().join(SOURCE_ROOT).join(".terlan");
        fs::create_dir(&cache).expect("interface cache");
        for support in &case.support_sources {
            let module = source_module(support).expect("support module");
            fs::write(cache.join(format!("{module}.typi")), &support.source)
                .expect("support interface");
        }
        (root, case)
    }

    #[test]
    fn support_modules_are_sorted_unique_and_syntax_checked() {
        assert_eq!(
            support_modules(&[
                source("src/Z.terl", "module demo.Z.\n"),
                source("src/A.terl", "module demo.A.\n"),
            ]),
            Ok(vec!["demo.A".into(), "demo.Z".into()])
        );
        assert!(support_modules(&[
            source("src/A.terl", "module demo.A.\n"),
            source("src/B.terl", "module demo.A.\n"),
        ])
        .unwrap_err()
        .contains("duplicate generated support module"));
        for invalid in [
            "pub value(): Int -> 1.\n",
            "module .\n",
            "module demo..A.\n",
            "module demo.A-B.\n",
            "module demo.1A.\n",
        ] {
            assert!(support_modules(&[source("src/Invalid.terl", invalid)])
                .unwrap_err()
                .contains("has no valid module declaration"));
        }
    }

    #[test]
    fn every_oracle_has_stable_roundtrip_spelling() {
        assert_eq!(oracle_name(Oracle::Check), "check");
        assert_eq!(oracle_name(Oracle::JavaScript), "js");
        assert_eq!(oracle_name(Oracle::Native), "native");
        assert_eq!(oracle_name(Oracle::None), "none");
    }

    #[cfg(unix)]
    #[test]
    fn interface_only_consumer_records_success_and_failure() {
        let (root, case) = prepared_case(Oracle::Check);
        run(
            root.path(),
            &case,
            Path::new("/bin/true"),
            1_000,
            1 << 30,
            1_000,
        )
        .expect("interface semantic comparison");
        let evidence =
            fs::read_to_string(root.path().join(ROUNDTRIP_ARTIFACT)).expect("round-trip evidence");
        assert!(evidence.contains("source_and_interface_consumers_accepted"));
        assert!(evidence.contains("\"interface_outcome\":\"passed\""));
        assert!(evidence.contains("type_fuzz.case000017.Batch"));
        assert!(!fs::read_dir(root.path())
            .expect("case entries")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".interface-roundtrip-")));

        let error = run(
            root.path(),
            &case,
            Path::new("/bin/false"),
            1_000,
            1 << 30,
            1_000,
        )
        .expect_err("failed interface compiler");
        assert!(error.contains("interface-only semantic comparison ended as failed"));
        assert!(fs::read_to_string(root.path().join(ROUNDTRIP_ARTIFACT))
            .expect("failed evidence")
            .contains("\"interface_outcome\":\"failed\""));

        let missing = support_modules(&case.support_sources).expect("support modules")[0].clone();
        fs::remove_file(
            root.path()
                .join(SOURCE_ROOT)
                .join(".terlan")
                .join(format!("{missing}.typi")),
        )
        .expect("remove generated interface");
        let error = run(
            root.path(),
            &case,
            Path::new("/bin/false"),
            1_000,
            1 << 30,
            1_000,
        )
        .expect_err("failed interface generation");
        assert!(error.contains("interface generation ended as failed"));
        assert!(fs::read_to_string(root.path().join(ROUNDTRIP_ARTIFACT))
            .expect("generation evidence")
            .contains("\"interface_outcome\":\"not_run\""));
    }

    #[cfg(unix)]
    #[test]
    fn missing_interfaces_are_generated_once_before_consumption() {
        use std::os::unix::fs::PermissionsExt;

        let (root, case) = prepared_case(Oracle::Check);
        let modules = support_modules(&case.support_sources).expect("support modules");
        for module in &modules {
            fs::remove_file(
                root.path()
                    .join(SOURCE_ROOT)
                    .join(".terlan")
                    .join(format!("{module}.typi")),
            )
            .expect("remove prepared interface");
        }
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            concat!(
                "#!/bin/sh\n",
                "find src -name '*.terl' -type f 2>/dev/null | while read -r source; do\n",
                "  module=$(sed -n 's/^module \\([^ ]*\\)\\.$/\\1/p' \"$source\")\n",
                "  test -n \"$module\" || continue\n",
                "  mkdir -p src/.terlan\n",
                "  cp \"$source\" \"src/.terlan/$module.typi\"\n",
                "done\n",
            ),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");

        run(root.path(), &case, &compiler, 1_000, 1 << 30, 1_000)
            .expect("generated interface comparison");
        assert!(modules.iter().all(|module| root
            .path()
            .join(SOURCE_ROOT)
            .join(".terlan")
            .join(format!("{module}.typi"))
            .is_file()));
    }

    #[test]
    fn inert_cases_do_not_require_an_interface_workspace() {
        let root = tempfile::tempdir().expect("case root");
        let mut no_modules = generated_case(Oracle::Check);
        no_modules.support_sources.clear();
        run(root.path(), &no_modules, Path::new("missing"), 1, 1, 1).expect("single-module case");
        run(
            root.path(),
            &generated_case(Oracle::None),
            Path::new("missing"),
            1,
            1,
            1,
        )
        .expect("offline case");
        assert!(!root.path().join(ROUNDTRIP_ARTIFACT).exists());
    }

    #[test]
    fn staging_attributes_every_filesystem_boundary() {
        let (root, case) = prepared_case(Oracle::Check);
        let modules = support_modules(&case.support_sources).expect("modules");

        let workspace_file = root.path().join("workspace-file");
        fs::write(&workspace_file, "blocked").expect("workspace blocker");
        assert!(run(&workspace_file, &case, Path::new("missing"), 1, 1, 1)
            .unwrap_err()
            .contains("create interface round-trip workspace"));

        let (missing_manifest_root, missing_manifest_case) = prepared_case(Oracle::Check);
        fs::remove_file(missing_manifest_root.path().join(PROJECT_MANIFEST))
            .expect("remove manifest");
        assert!(run(
            missing_manifest_root.path(),
            &missing_manifest_case,
            Path::new("/bin/true"),
            1_000,
            1 << 30,
            1_000,
        )
        .unwrap_err()
        .contains("copy interface round-trip input `terlan.toml`"));

        let cache_blocked = tempfile::tempdir().expect("cache blocked");
        fs::write(cache_blocked.path().join(".terlan"), "blocked").expect("cache blocker");
        assert!(stage(
            cache_blocked.path(),
            root.path(),
            &root.path().join(SOURCE_ROOT).join(".terlan"),
            &modules,
        )
        .unwrap_err()
        .contains("create interface round-trip cache"));

        let source_blocked = tempfile::tempdir().expect("source blocked");
        fs::write(source_blocked.path().join("src"), "blocked").expect("source blocker");
        assert!(stage(
            source_blocked.path(),
            root.path(),
            &root.path().join(SOURCE_ROOT).join(".terlan"),
            &modules,
        )
        .unwrap_err()
        .contains("create interface round-trip source root"));

        let missing_manifest = tempfile::tempdir().expect("missing manifest");
        assert!(stage(
            missing_manifest.path(),
            Path::new("missing"),
            &root.path().join(SOURCE_ROOT).join(".terlan"),
            &modules,
        )
        .unwrap_err()
        .contains("copy interface round-trip input `terlan.toml`"));

        let missing_entry_root = tempfile::tempdir().expect("missing entry root");
        fs::write(
            missing_entry_root.path().join(PROJECT_MANIFEST),
            &case.project,
        )
        .expect("manifest");
        let missing_entry_workspace = tempfile::tempdir().expect("missing entry workspace");
        assert!(stage(
            missing_entry_workspace.path(),
            missing_entry_root.path(),
            &root.path().join(SOURCE_ROOT).join(".terlan"),
            &modules,
        )
        .unwrap_err()
        .contains("copy interface round-trip input `CaseTest.terl`"));

        let missing_interface = tempfile::tempdir().expect("missing interface");
        let absent = root
            .path()
            .join(SOURCE_ROOT)
            .join(".terlan")
            .join(format!("{}.typi", modules[0]));
        fs::remove_file(&absent).expect("remove interface");
        assert!(stage(
            missing_interface.path(),
            root.path(),
            &root.path().join(SOURCE_ROOT).join(".terlan"),
            &modules,
        )
        .unwrap_err()
        .contains("copy interface round-trip input"));
        assert!(run(
            root.path(),
            &case,
            Path::new("/bin/true"),
            1_000,
            1 << 30,
            1_000,
        )
        .unwrap_err()
        .contains("copy interface round-trip input"));

        let mut invalid_module = case.clone();
        invalid_module.support_sources[0].source = "not a module".into();
        assert!(
            run(root.path(), &invalid_module, Path::new("missing"), 1, 1, 1,)
                .unwrap_err()
                .contains("has no valid module declaration")
        );
    }

    #[cfg(unix)]
    #[test]
    fn evidence_publication_failure_is_attributed() {
        let (root, case) = prepared_case(Oracle::Check);
        fs::create_dir(root.path().join(format!("{ROUNDTRIP_ARTIFACT}.tmp")))
            .expect("publication blocker");
        assert!(run(
            root.path(),
            &case,
            Path::new("/bin/true"),
            1_000,
            1 << 30,
            1_000
        )
        .unwrap_err()
        .contains("write temporary fuzz artifact"));

        let (root, case) = prepared_case(Oracle::Check);
        let missing = support_modules(&case.support_sources).expect("support modules")[0].clone();
        fs::remove_file(
            root.path()
                .join(SOURCE_ROOT)
                .join(".terlan")
                .join(format!("{missing}.typi")),
        )
        .expect("remove generated interface");
        fs::create_dir(root.path().join(format!("{ROUNDTRIP_ARTIFACT}.tmp")))
            .expect("generation publication blocker");
        assert!(run(
            root.path(),
            &case,
            Path::new("/bin/false"),
            1_000,
            1 << 30,
            1_000
        )
        .unwrap_err()
        .contains("write temporary fuzz artifact"));
    }
}
