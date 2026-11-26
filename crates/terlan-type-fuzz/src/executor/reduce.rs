//! Bounded reduction of deterministic generated failures.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

#[cfg(test)]
use super::generate_case;
use super::{artifacts, oracle, regenerate_case};
use crate::{CaseManifest, GeneratedCase, RunOptions};

const MAX_PROBES: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
struct FailureClass {
    outcome: String,
    diagnostic: String,
}

pub(super) fn run(directory: &Path, compiler: &Path) -> Result<(), String> {
    let original = artifacts::load_generated_case(directory)?;
    run_loaded(directory, original, &mut |case| {
        execute_case(case, compiler)
    })
}

fn run_loaded(
    directory: &Path,
    original: GeneratedCase,
    execute: &mut dyn FnMut(&GeneratedCase) -> Result<oracle::OracleResult, String>,
) -> Result<(), String> {
    let baseline = execute(&original)?;
    let class = failure_class(&baseline).ok_or_else(|| {
        "type fuzz reduction requires a reproducible compiler failure".to_string()
    })?;
    let candidates = reduction_candidates(&original);
    let mut selected = original.clone();
    let mut selected_result = baseline;
    let mut probes = 0_usize;
    for candidate in candidates.into_iter().take(MAX_PROBES) {
        probes += 1;
        let result = execute(&candidate)?;
        if failure_class(&result).as_ref() == Some(&class) {
            selected = candidate;
            selected_result = result;
            break;
        }
    }
    artifacts::write_case(directory, &selected)?;
    artifacts::record_observation(directory, &selected_result, &selected.manifest)?;
    let report = serde_json::json!({
        "schema": "terlan.type-fuzz-reduction.v1",
        "original_case_id": original.manifest.case_id,
        "reduced_case_id": selected.manifest.case_id,
        "failure": {
            "outcome": class.outcome,
            "diagnostic": class.diagnostic,
        },
        "probes": probes,
        "source_bytes_before": source_bytes(&original),
        "source_bytes_after": source_bytes(&selected),
    });
    let mut bytes = report.to_string().into_bytes();
    bytes.push(b'\n');
    artifacts::atomic_write(&directory.join("reduction.json"), &bytes)?;
    println!(
        "reduced {} to {} in {probes} probes",
        original.manifest.case_id, selected.manifest.case_id
    );
    Ok(())
}

fn execute_case(case: &GeneratedCase, compiler: &Path) -> Result<oracle::OracleResult, String> {
    execute_case_with(case, compiler, tempfile::tempdir)
}

fn execute_case_with(
    case: &GeneratedCase,
    compiler: &Path,
    temporary: fn() -> io::Result<tempfile::TempDir>,
) -> Result<oracle::OracleResult, String> {
    let root = temporary().map_err(|error| format!("create reduction case: {error}"))?;
    artifacts::write_case(root.path(), case)?;
    Ok(oracle::run(
        compiler,
        case.manifest.oracle,
        &root.path().join("CaseTest.terl"),
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    ))
}

fn failure_class(result: &oracle::OracleResult) -> Option<FailureClass> {
    if !matches!(result.outcome, "failed" | "resource_exhausted") {
        return None;
    }
    let diagnostic = result
        .stderr
        .lines()
        .find_map(|line| {
            let start = line.find("error[")?;
            let suffix = &line[start..];
            let end = suffix.find(']')?;
            Some(suffix[..=end].to_string())
        })
        .unwrap_or_else(|| "unclassified".to_string());
    Some(FailureClass {
        outcome: result.outcome.to_string(),
        diagnostic,
    })
}

fn reduction_candidates(original: &GeneratedCase) -> Vec<GeneratedCase> {
    let manifest = &original.manifest;
    let mut candidates = Vec::new();
    for depth in 1..=manifest.max_depth {
        candidates.push(generate(
            manifest,
            manifest.seed,
            manifest.case_index,
            depth,
        ));
    }
    for index in 0..20 {
        candidates.push(generate(manifest, manifest.seed, index, manifest.max_depth));
    }
    for seed in 0..=128 {
        candidates.push(generate(
            manifest,
            seed,
            manifest.case_index,
            manifest.max_depth,
        ));
    }
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| {
        candidate.manifest.family == manifest.family
            && candidate.manifest.expected_type == manifest.expected_type
            && candidate != original
            && seen.insert(candidate.manifest.case_id.clone())
    });
    candidates.sort_by_key(score);
    candidates
}

fn generate(manifest: &CaseManifest, seed: u64, index: u32, max_depth: u8) -> GeneratedCase {
    let options = RunOptions {
        seed,
        cases: 1,
        max_depth,
        oracle: manifest.oracle,
        case_timeout_ms: manifest.case_timeout_ms,
        rss_limit_bytes: manifest.rss_limit_bytes,
        cpu_limit_ms: manifest.cpu_limit_ms,
        ..RunOptions::default()
    };
    regenerate_case(
        index,
        &options,
        &manifest.compiler_revision,
        &manifest.generator,
    )
    .expect("loaded manifests use supported fuzz generators")
}

fn score(case: &GeneratedCase) -> (usize, u32, u8, u64) {
    (
        source_bytes(case),
        case.manifest.case_index,
        case.manifest.max_depth,
        case.manifest.seed,
    )
}

fn source_bytes(case: &GeneratedCase) -> usize {
    case.source.len()
        + case
            .support_sources
            .iter()
            .map(|source| source.source.len())
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Oracle;
    use std::fs;

    fn case(index: u32, seed: u64, oracle: Oracle) -> GeneratedCase {
        let options = RunOptions {
            seed,
            cases: 1,
            oracle,
            ..RunOptions::default()
        };
        generate_case(index, &options, "reducer-test")
    }

    #[test]
    fn candidates_are_same_family_unique_and_sorted() {
        let original = case(16, u64::MAX, Oracle::Check);
        let candidates = reduction_candidates(&original);
        assert!(!candidates.is_empty());
        assert!(candidates
            .windows(2)
            .all(|pair| score(&pair[0]) <= score(&pair[1])));
        assert!(candidates.iter().all(|candidate| {
            candidate.manifest.family == original.manifest.family
                && candidate.manifest.expected_type == original.manifest.expected_type
                && candidate != &original
        }));
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| &candidate.manifest.case_id)
                .collect::<BTreeSet<_>>()
                .len(),
            candidates.len()
        );

        let multi_module = case(17, 1, Oracle::Check);
        assert_eq!(
            source_bytes(&multi_module),
            multi_module.source.len()
                + multi_module
                    .support_sources
                    .iter()
                    .map(|source| source.source.len())
                    .sum::<usize>()
        );
        assert!(source_bytes(&multi_module) > multi_module.source.len());
    }

    #[test]
    fn failure_classes_use_stable_diagnostic_codes() {
        let mut result = oracle::run(
            Path::new("missing"),
            Oracle::None,
            Path::new("case.terl"),
            1,
            1,
            1,
        );
        assert_eq!(failure_class(&result), None);
        result.outcome = "failed";
        result.stderr = "prefix error[type_mismatch]: detail".into();
        assert_eq!(
            failure_class(&result),
            Some(FailureClass {
                outcome: "failed".into(),
                diagnostic: "error[type_mismatch]".into(),
            })
        );
        result.outcome = "resource_exhausted";
        result.stderr.clear();
        assert_eq!(
            failure_class(&result).expect("resource failure").diagnostic,
            "unclassified"
        );
        result.stderr = "ordinary diagnostic\nerror[incomplete".into();
        assert_eq!(
            failure_class(&result)
                .expect("malformed diagnostics")
                .diagnostic,
            "unclassified"
        );
    }

    fn failed_result(diagnostic: &str) -> oracle::OracleResult {
        let mut result = oracle::run(
            Path::new("missing"),
            Oracle::None,
            Path::new("case.terl"),
            1,
            1,
            1,
        );
        result.outcome = "failed";
        result.stderr = diagnostic.into();
        result
    }

    #[test]
    fn reduction_attributes_execution_and_temporary_artifact_failures() {
        let root = tempfile::tempdir().expect("reduction root");
        let original = case(16, u64::MAX, Oracle::Check);
        assert!(run_loaded(root.path(), original.clone(), &mut |_| {
            Err("baseline".into())
        })
        .unwrap_err()
        .contains("baseline"));

        let mut calls = 0;
        assert!(run_loaded(root.path(), original.clone(), &mut |_| {
            calls += 1;
            if calls == 1 {
                Ok(failed_result("error[baseline]: original"))
            } else {
                Err("candidate".into())
            }
        })
        .unwrap_err()
        .contains("candidate"));

        assert!(execute_case_with(&original, Path::new("missing"), || {
            Err(io::Error::other("temporary root"))
        })
        .unwrap_err()
        .contains("create reduction case"));
        assert!(execute_case_with(&original, Path::new("missing"), || {
            let directory = tempfile::tempdir().expect("temporary case");
            fs::create_dir(directory.path().join("terlan.toml.tmp")).expect("artifact blocker");
            Ok(directory)
        })
        .unwrap_err()
        .contains("write temporary"));
    }

    #[cfg(unix)]
    #[test]
    fn reducer_keeps_the_failure_class_and_rejects_passing_cases() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("reduction root");
        let directory = root.path().join("case");
        fs::create_dir(&directory).expect("case directory");
        let original = case(16, u64::MAX, Oracle::Check);
        artifacts::write_case(&directory, &original).expect("original case");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            "#!/bin/sh\necho 'error[type_mismatch]: stable failure' >&2\nexit 1\n",
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");

        run(&directory, &compiler).expect("reduce failure");
        let reduced = artifacts::load_generated_case(&directory).expect("reduced case");
        assert_eq!(reduced.manifest.family, original.manifest.family);
        assert!(score(&reduced) < score(&original));
        assert!(directory.join("reduction.json").is_file());

        let passing = root.path().join("passing");
        fs::create_dir(&passing).expect("passing directory");
        artifacts::write_case(&passing, &case(1, 1, Oracle::Check)).expect("passing case");
        assert!(run(&passing, Path::new("/bin/true"))
            .unwrap_err()
            .contains("reproducible compiler failure"));
    }

    #[cfg(unix)]
    #[test]
    fn reducer_retains_original_when_candidates_change_failure_class() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("reduction root");
        let directory = root.path().join("case");
        fs::create_dir(&directory).expect("case directory");
        let original = case(16, u64::MAX, Oracle::Check);
        artifacts::write_case(&directory, &original).expect("original case");
        let fixture = root.path().join("original");
        fs::create_dir(&fixture).expect("original fixture directory");
        artifacts::write_case(&fixture, &original).expect("original fixture");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            "#!/bin/sh\nif cmp -s case.toml \"$(dirname \"$0\")/original/case.toml\"; then\n  echo 'error[baseline]: original' >&2\nelse\n  echo 'error[candidate]: changed' >&2\nfi\nexit 1\n",
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");

        run(&directory, &compiler).expect("bounded reduction");
        let retained = artifacts::load_generated_case(&directory).expect("retained case");
        assert_eq!(retained, original);
        let report = fs::read_to_string(directory.join("reduction.json")).expect("report");
        assert!(report.contains("\"probes\":8"));
    }

    #[cfg(unix)]
    #[test]
    fn reducer_attributes_every_publication_failure() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("reduction root");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            "#!/bin/sh\necho 'error[stable]: failure' >&2\nexit 1\n",
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        for blocker in ["terlan.toml.tmp", "observed.json.tmp", "reduction.json.tmp"] {
            let directory = root.path().join(blocker.replace('.', "-"));
            fs::create_dir(&directory).expect("case directory");
            let original = case(16, u64::MAX, Oracle::Check);
            artifacts::write_case(&directory, &original).expect("original case");
            fs::create_dir(directory.join(blocker)).expect("publication blocker");
            assert!(run(&directory, &compiler)
                .unwrap_err()
                .contains("write temporary"));
        }
    }
}
