use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::generate_case as generate;
use super::oracle;
use terlan_type_fuzz::{CaseManifest, GeneratedCase, RunOptions, CASE_SCHEMA};

pub(super) fn generate_run(options: &RunOptions) -> Result<(), String> {
    fs::create_dir_all(&options.out_dir).map_err(io_error("create fuzz output"))?;
    let started = Instant::now();
    for index in 0..options.cases {
        if started.elapsed() >= Duration::from_millis(options.total_timeout_ms) {
            return Err(format!(
                "run exceeded {} ms after {index} cases",
                options.total_timeout_ms
            ));
        }
        let case = generate(index, options);
        let directory = options.out_dir.join(&case.manifest.case_id);
        write_case(&directory, &case)?;
        run_and_record(&directory, &case)?;
        println!("{} {}", case.manifest.case_id, case.manifest.family);
    }
    Ok(())
}

pub(super) fn replay(directory: &Path) -> Result<(), String> {
    let manifest_path = directory.join("case.toml");
    let text = fs::read_to_string(&manifest_path).map_err(io_error("read case manifest"))?;
    let manifest: CaseManifest = toml::from_str(&text)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    if manifest.schema != CASE_SCHEMA {
        return Err(format!("unsupported case schema `{}`", manifest.schema));
    }
    let options = RunOptions {
        seed: manifest.seed,
        max_depth: manifest.max_depth,
        oracle: manifest.oracle,
        case_timeout_ms: manifest.case_timeout_ms,
        rss_limit_bytes: manifest.rss_limit_bytes,
        cpu_limit_ms: manifest.cpu_limit_ms,
        ..RunOptions::default()
    };
    let mut case = generate(manifest.case_index, &options);
    case.manifest.compiler_version = manifest.compiler_version.clone();
    case.manifest.compiler_revision = manifest.compiler_revision.clone();
    verify_case(directory, &case)?;
    run_and_record(directory, &case)?;
    println!("replayed {}", manifest.case_id);
    Ok(())
}

fn write_case(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(io_error("create case directory"))?;
    atomic_write(&directory.join("Cargo.toml"), case.project.as_bytes())?;
    atomic_write(&directory.join("case.terl"), case.source.as_bytes())?;
    atomic_write(
        &directory.join("case.toml"),
        toml_text(&case.manifest)?.as_bytes(),
    )?;
    atomic_json(&directory.join("expected.json"), &case.expected)?;
    atomic_json(
        &directory.join("timings.json"),
        &serde_json::json!({
            "schema": "terlan.type-fuzz-timings.v1",
            "measurement": "not_recorded_tf0"
        }),
    )?;
    Ok(())
}

fn run_and_record(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    let result = oracle::run(
        case.manifest.oracle,
        &directory.join("case.terl"),
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    )?;
    atomic_json(&directory.join("observed.json"), &result)?;
    atomic_json(
        &directory.join("diagnostics.json"),
        &serde_json::json!({
            "schema": "terlan.type-fuzz-diagnostics.v1",
            "stdout": &result.stdout,
            "stderr": &result.stderr
        }),
    )?;
    let resources = if matches!(result.outcome, "passed" | "not_run") {
        result.resources.deterministic_success()
    } else {
        result.resources.clone()
    };
    atomic_json(&directory.join("resources.json"), &resources)?;
    if !matches!(result.outcome, "passed" | "not_run") {
        return Err(format!(
            "case {} check oracle ended as {}",
            case.manifest.case_id, result.outcome
        ));
    }
    Ok(())
}

fn verify_case(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    let expected = [
        ("Cargo.toml", case.project.as_bytes().to_vec()),
        ("case.terl", case.source.as_bytes().to_vec()),
        ("case.toml", toml_text(&case.manifest)?.into_bytes()),
        ("expected.json", json_bytes(&case.expected)?),
    ];
    for (name, bytes) in expected {
        let path = directory.join(name);
        let actual = fs::read(&path).map_err(io_error("read replay artifact"))?;
        if actual != bytes {
            return Err(format!("replay input drifted: {}", path.display()));
        }
    }
    Ok(())
}

fn toml_text(manifest: &CaseManifest) -> Result<String, String> {
    let quote = |value: &str| {
        serde_json::to_string(value).map_err(|error| format!("encode manifest string: {error}"))
    };
    Ok(format!(
        concat!(
            "schema = {}\n",
            "generator = {}\n",
            "compiler_version = {}\n",
            "compiler_revision = {}\n",
            "case_id = {}\n",
            "seed = {}\n",
            "case_index = {}\n",
            "max_depth = {}\n",
            "family = {}\n",
            "expected_type = {}\n",
            "oracle = {}\n",
            "case_timeout_ms = {}\n",
            "rss_limit_bytes = {}\n",
            "cpu_limit_ms = {}\n"
        ),
        quote(&manifest.schema)?,
        quote(&manifest.generator)?,
        quote(&manifest.compiler_version)?,
        quote(&manifest.compiler_revision)?,
        quote(&manifest.case_id)?,
        manifest.seed,
        manifest.case_index,
        manifest.max_depth,
        quote(&manifest.family)?,
        quote(&manifest.expected_type)?,
        quote(match manifest.oracle {
            terlan_type_fuzz::Oracle::Check => "check",
            terlan_type_fuzz::Oracle::None => "none",
        })?,
        manifest.case_timeout_ms,
        manifest.rss_limit_bytes,
        manifest.cpu_limit_ms,
    ))
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    atomic_write(path, &json_bytes(value)?)
}

fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|error| format!("encode JSON: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = temporary_path(path);
    fs::write(&temporary, bytes).map_err(io_error("write temporary fuzz artifact"))?;
    fs::rename(&temporary, path).map_err(io_error("publish fuzz artifact"))
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

fn io_error(action: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("{action}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use terlan_type_fuzz::Oracle;

    #[test]
    fn case_artifacts_are_byte_identical_across_writes() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let case = generate(3, &test_options(5));
        write_case(root.path(), &case).expect("first write");
        let first = fs::read(root.path().join("case.toml")).expect("manifest");
        write_case(root.path(), &case).expect("second write");
        assert_eq!(
            first,
            fs::read(root.path().join("case.toml")).expect("manifest")
        );
        verify_case(root.path(), &case).expect("replay identity");
    }

    #[test]
    fn replay_rejects_source_drift() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let case = generate(0, &test_options(5));
        write_case(root.path(), &case).expect("write");
        fs::write(root.path().join("case.terl"), "changed").expect("change source");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("drifted"));
    }

    #[test]
    fn disabled_oracle_run_can_be_replayed_offline() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 2,
            oracle: Oracle::None,
            out_dir: root.path().to_path_buf(),
            ..RunOptions::default()
        };
        generate_run(&options).expect("generate run");
        let case = generate(1, &options);
        let directory = root.path().join(case.manifest.case_id);
        let names = [
            "Cargo.toml",
            "case.terl",
            "case.toml",
            "expected.json",
            "observed.json",
            "diagnostics.json",
            "timings.json",
            "resources.json",
        ];
        let before = names
            .iter()
            .map(|name| fs::read(directory.join(name)).expect("generated artifact"))
            .collect::<Vec<_>>();
        replay(&directory).expect("offline replay");
        let after = names
            .iter()
            .map(|name| fs::read(directory.join(name)).expect("replayed artifact"))
            .collect::<Vec<_>>();
        assert_eq!(before, after);
    }

    fn test_options(seed: u64) -> RunOptions {
        RunOptions {
            seed,
            oracle: Oracle::None,
            case_timeout_ms: 2_000,
            rss_limit_bytes: 1024,
            cpu_limit_ms: 2_000,
            ..RunOptions::default()
        }
    }
}
