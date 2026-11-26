use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::generate_case as generate;
use super::oracle::{self, OracleResult};
use super::resources::ResourceEvidence;
use crate::{CaseManifest, GeneratedCase, Oracle, RunOptions, TypedValue, CASE_SCHEMA};

const CHECK_BATCH_SIZE: usize = 32;
const CASE_SOURCE_FILE: &str = "CaseTest.terl";

pub(super) fn generate_run(
    options: &RunOptions,
    compiler: &Path,
    compiler_revision: &str,
) -> Result<(), String> {
    let started = Instant::now();
    let mut elapsed = || started.elapsed();
    generate_run_with_elapsed(options, compiler, compiler_revision, &mut elapsed)
}

fn generate_run_with_elapsed(
    options: &RunOptions,
    compiler: &Path,
    compiler_revision: &str,
    elapsed: &mut dyn FnMut() -> Duration,
) -> Result<(), String> {
    fs::create_dir_all(&options.out_dir).map_err(io_error("create fuzz output"))?;
    let mut pending = Vec::with_capacity(options.cases as usize);
    for index in 0..options.cases {
        if elapsed() >= Duration::from_millis(options.total_timeout_ms) {
            return Err(format!(
                "run exceeded {} ms after {index} cases",
                options.total_timeout_ms
            ));
        }
        let case = generate(index, options, compiler_revision);
        let directory = options.out_dir.join(&case.manifest.case_id);
        write_case(&directory, &case)?;
        pending.push((directory, case));
    }
    run_batches(options, compiler, &pending, elapsed)?;
    for (_, case) in pending {
        println!("{} {}", case.manifest.case_id, case.manifest.family);
    }
    Ok(())
}

fn run_batches(
    options: &RunOptions,
    compiler: &Path,
    pending: &[(PathBuf, GeneratedCase)],
    elapsed: &mut dyn FnMut() -> Duration,
) -> Result<(), String> {
    for batch in pending.chunks(CHECK_BATCH_SIZE) {
        let spent_ms = u64::try_from(elapsed().as_millis()).unwrap_or(u64::MAX);
        let remaining_ms = options
            .total_timeout_ms
            .checked_sub(spent_ms)
            .filter(|remaining| *remaining > 0)
            .ok_or_else(|| "run exceeded total timeout before compiler oracle".to_string())?;
        let batch_cases = batch.len() as u64;
        let wall_ms = remaining_ms.min(options.case_timeout_ms.saturating_mul(batch_cases));
        let cpu_ms = options.cpu_limit_ms.saturating_mul(batch_cases);
        let sources = batch
            .iter()
            .map(|(directory, _)| {
                directory
                    .strip_prefix(&options.out_dir)
                    .expect("case directory belongs to fuzz output")
                    .join(CASE_SOURCE_FILE)
            })
            .collect::<Vec<_>>();
        let result = oracle::run_batch(
            compiler,
            options.oracle,
            &options.out_dir,
            &sources,
            wall_ms,
            options.rss_limit_bytes,
            cpu_ms,
        );
        for (directory, case) in batch {
            record_observation(directory, &result, &case.manifest)?;
        }
        ensure_success(&batch[0].1, &result)?;
    }
    Ok(())
}

pub(super) fn replay(directory: &Path, compiler: &Path) -> Result<(), String> {
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
    let case = generate(manifest.case_index, &options, &manifest.compiler_revision);
    verify_case(directory, &case)?;
    run_and_record(directory, &case, compiler)?;
    println!("replayed {}", manifest.case_id);
    Ok(())
}

fn write_case(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(io_error("create case directory"))?;
    atomic_write(&directory.join("Cargo.toml"), case.project.as_bytes())?;
    atomic_write(&directory.join(CASE_SOURCE_FILE), case.source.as_bytes())?;
    atomic_write(&directory.join("replay.terl"), case.source.as_bytes())?;
    let manifest = toml_text(&case.manifest);
    atomic_write(&directory.join("case.toml"), manifest.as_bytes())?;
    atomic_write(
        &directory.join("expected.json"),
        &json_bytes(&typed_value_json(&case.expected)),
    )?;
    let timings = serde_json::json!({
        "schema": "terlan.type-fuzz-timings.v1",
        "measurement": "not_recorded_tf0"
    });
    atomic_write(&directory.join("timings.json"), &json_bytes(&timings))?;
    Ok(())
}

fn run_and_record(directory: &Path, case: &GeneratedCase, compiler: &Path) -> Result<(), String> {
    let result = oracle::run(
        compiler,
        case.manifest.oracle,
        &directory.join(CASE_SOURCE_FILE),
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    );
    record_observation(directory, &result, &case.manifest)?;
    ensure_success(case, &result)
}

fn record_observation(
    directory: &Path,
    result: &OracleResult,
    manifest: &CaseManifest,
) -> Result<(), String> {
    atomic_write(
        &directory.join("observed.json"),
        &json_bytes(&oracle_result_json(result)),
    )?;
    let diagnostics = serde_json::json!({
        "schema": "terlan.type-fuzz-diagnostics.v1",
        "stdout": &result.stdout,
        "stderr": &result.stderr
    });
    atomic_write(
        &directory.join("diagnostics.json"),
        &json_bytes(&diagnostics),
    )?;
    atomic_write(
        &directory.join("timings.json"),
        &json_bytes(&timing_evidence_json(result)),
    )?;
    let resources = if result.oracle == Oracle::Native && result.outcome == "passed" {
        result.resources.clone()
    } else if matches!(result.outcome, "passed" | "not_run") {
        ResourceEvidence::unmeasured(
            manifest.case_timeout_ms,
            manifest.rss_limit_bytes,
            manifest.cpu_limit_ms,
        )
        .deterministic_success()
    } else {
        result.resources.clone()
    };
    atomic_write(
        &directory.join("resources.json"),
        &json_bytes(&resource_evidence_json(
            &resources,
            last_compiler_phase(&result.stderr),
        )),
    )?;
    Ok(())
}

fn ensure_success(case: &GeneratedCase, result: &OracleResult) -> Result<(), String> {
    if !matches!(result.outcome, "passed" | "not_run") {
        return Err(format!(
            "case {} {} oracle ended as {}",
            case.manifest.case_id,
            oracle_name(case.manifest.oracle),
            result.outcome
        ));
    }
    Ok(())
}

fn verify_case(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    let expected = [
        ("Cargo.toml", case.project.as_bytes().to_vec()),
        (CASE_SOURCE_FILE, case.source.as_bytes().to_vec()),
        ("replay.terl", case.source.as_bytes().to_vec()),
        ("case.toml", toml_text(&case.manifest).into_bytes()),
        (
            "expected.json",
            json_bytes(&typed_value_json(&case.expected)),
        ),
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

fn toml_text(manifest: &CaseManifest) -> String {
    let oracle = oracle_name(manifest.oracle);
    format!(
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
        quote(&manifest.schema),
        quote(&manifest.generator),
        quote(&manifest.compiler_version),
        quote(&manifest.compiler_revision),
        quote(&manifest.case_id),
        manifest.seed,
        manifest.case_index,
        manifest.max_depth,
        quote(&manifest.family),
        quote(&manifest.expected_type),
        quote(oracle),
        manifest.case_timeout_ms,
        manifest.rss_limit_bytes,
        manifest.cpu_limit_ms,
    )
}

fn quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\u{0008}' => quoted.push_str("\\b"),
            '\u{000c}' => quoted.push_str("\\f"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if control <= '\u{001f}' => {
                quoted.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            ordinary => quoted.push(ordinary),
        }
    }
    quoted.push('"');
    quoted
}

fn json_bytes(value: &serde_json::Value) -> Vec<u8> {
    let mut bytes = value.to_string().into_bytes();
    bytes.push(b'\n');
    bytes
}

fn typed_value_json(value: &TypedValue) -> serde_json::Value {
    serde_json::Value::Object(
        [
            ("schema".into(), value.schema.into()),
            ("type".into(), value.type_name.clone().into()),
            ("value".into(), value.value.clone()),
        ]
        .into_iter()
        .collect(),
    )
}

fn oracle_result_json(result: &OracleResult) -> serde_json::Value {
    serde_json::Value::Object(
        [
            ("schema".into(), result.schema.into()),
            ("oracle".into(), oracle_name(result.oracle).into()),
            ("outcome".into(), result.outcome.into()),
            (
                "exit_code".into(),
                result.exit_code.map_or(serde_json::Value::Null, Into::into),
            ),
            ("stdout".into(), result.stdout.clone().into()),
            ("stderr".into(), result.stderr.clone().into()),
        ]
        .into_iter()
        .collect(),
    )
}

fn oracle_name(oracle: Oracle) -> &'static str {
    match oracle {
        Oracle::Check => "check",
        Oracle::Native => "native",
        Oracle::None => "none",
    }
}

fn timing_evidence_json(result: &OracleResult) -> serde_json::Value {
    let events = result
        .stderr
        .lines()
        .filter(|line| line.starts_with("terlc timing:") || line.starts_with("terlc native-aot:"))
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema": "terlan.type-fuzz-timings.v1",
        "measurement": if result.oracle == Oracle::Native {
            "compiler_events"
        } else {
            "not_requested"
        },
        "events": events
    })
}

fn timing_phase(line: &str) -> Option<&str> {
    let event = line
        .strip_prefix("terlc timing: ")
        .or_else(|| line.strip_prefix("terlc native-aot: "))?;
    event.split_once(':').map(|(phase, _)| phase)
}

fn last_compiler_phase(stderr: &str) -> Option<&str> {
    stderr.lines().filter_map(timing_phase).next_back()
}

fn resource_evidence_json(
    evidence: &ResourceEvidence,
    last_compiler_phase: Option<&str>,
) -> serde_json::Value {
    let optional_u64 = |value: Option<u64>| value.map_or(serde_json::Value::Null, Into::into);
    serde_json::Value::Object(
        [
            ("schema".into(), evidence.schema.into()),
            (
                "measurement_supported".into(),
                evidence.measurement_supported.into(),
            ),
            ("wall_timeout_ms".into(), evidence.wall_timeout_ms.into()),
            (
                "wall_elapsed_ms".into(),
                optional_u64(evidence.wall_elapsed_ms),
            ),
            (
                "output_limit_bytes".into(),
                evidence.output_limit_bytes.into(),
            ),
            ("rss_limit_bytes".into(), evidence.rss_limit_bytes.into()),
            ("cpu_limit_ms".into(), evidence.cpu_limit_ms.into()),
            (
                "peak_rss_bytes".into(),
                optional_u64(evidence.peak_rss_bytes),
            ),
            ("peak_cpu_ms".into(), optional_u64(evidence.peak_cpu_ms)),
            (
                "limit_exceeded".into(),
                evidence
                    .limit_exceeded
                    .map_or(serde_json::Value::Null, Into::into),
            ),
            (
                "last_compiler_phase".into(),
                last_compiler_phase.map_or(serde_json::Value::Null, Into::into),
            ),
            (
                "process_group_cleanup".into(),
                evidence.process_group_cleanup.into(),
            ),
        ]
        .into_iter()
        .collect(),
    )
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

    #[test]
    fn every_oracle_has_stable_artifact_spelling() {
        assert_eq!(oracle_name(Oracle::Check), "check");
        assert_eq!(oracle_name(Oracle::Native), "native");
        assert_eq!(oracle_name(Oracle::None), "none");
    }

    #[test]
    fn native_success_keeps_measured_resources() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let mut options = test_options(7);
        options.oracle = Oracle::Native;
        let case = generate(0, &options, "test");
        write_case(root.path(), &case).expect("case artifacts");
        let mut resources = ResourceEvidence::unmeasured(10, 20, 30);
        resources.peak_rss_bytes = Some(42);
        resources.peak_cpu_ms = Some(7);
        resources.wall_elapsed_ms = Some(9);
        let result = OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle: Oracle::Native,
            outcome: "passed",
            exit_code: Some(0),
            stdout: String::new(),
            stderr: concat!(
                "terlc timing: frontend.parse: +3ms total=3ms\n",
                "ordinary diagnostic\n",
                "terlc native-aot: image.native-ir: +5ms total=5ms\n"
            )
            .into(),
            resources,
        };
        record_observation(root.path(), &result, &case.manifest).expect("observation");
        let recorded =
            fs::read_to_string(root.path().join("resources.json")).expect("resource evidence");
        assert!(recorded.contains("\"peak_rss_bytes\":42"));
        assert!(recorded.contains("\"peak_cpu_ms\":7"));
        assert!(recorded.contains("\"wall_elapsed_ms\":9"));
        assert!(recorded.contains("\"last_compiler_phase\":\"image.native-ir\""));
        let timings =
            fs::read_to_string(root.path().join("timings.json")).expect("timing evidence");
        assert!(timings.contains("\"measurement\":\"compiler_events\""));
        assert!(timings.contains("frontend.parse"));
        assert!(timings.contains("image.native-ir"));
        assert!(!timings.contains("ordinary diagnostic"));
        assert_eq!(timing_phase("ordinary diagnostic"), None);
        assert_eq!(
            timing_phase("terlc timing: frontend.parse: +3ms total=3ms"),
            Some("frontend.parse")
        );
    }

    #[test]
    fn case_artifacts_are_byte_identical_across_writes() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let case = generate(3, &test_options(5), "test");
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
    fn manifest_strings_use_complete_basic_string_escaping() {
        assert_eq!(
            quote("\"\\\u{0008}\u{000c}\n\r\t\u{0001}λ"),
            "\"\\\"\\\\\\b\\f\\n\\r\\t\\u0001λ\""
        );
    }

    #[test]
    fn replay_rejects_schema_and_source_drift() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let case = generate(0, &test_options(5), "test");
        write_case(root.path(), &case).expect("write");
        fs::write(root.path().join(CASE_SOURCE_FILE), "changed").expect("change source");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("drifted"));
        assert!(replay(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("drifted"));

        write_case(root.path(), &case).expect("restore");
        let manifest = fs::read_to_string(root.path().join("case.toml"))
            .expect("manifest")
            .replace(CASE_SCHEMA, "future.schema");
        fs::write(root.path().join("case.toml"), manifest).expect("future manifest");
        assert!(replay(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("unsupported case schema"));

        fs::write(root.path().join("case.toml"), "not valid toml = [").expect("invalid manifest");
        assert!(replay(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("invalid"));
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
        generate_run(&options, Path::new("missing-compiler"), "test").expect("generate run");
        let case = generate(1, &options, "test");
        let directory = root.path().join(case.manifest.case_id);
        let names = [
            "Cargo.toml",
            CASE_SOURCE_FILE,
            "replay.terl",
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
        replay(&directory, Path::new("missing-compiler")).expect("offline replay");
        let after = names
            .iter()
            .map(|name| fs::read(directory.join(name)).expect("replayed artifact"))
            .collect::<Vec<_>>();
        assert_eq!(before, after);
    }

    #[test]
    fn total_run_deadline_is_enforced_before_generation() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 10,
            total_timeout_ms: 1,
            oracle: Oracle::None,
            out_dir: root.path().to_path_buf(),
            ..RunOptions::default()
        };
        let mut elapsed = || Duration::from_millis(2);
        let result = generate_run_with_elapsed(
            &options,
            Path::new("missing-compiler"),
            "test",
            &mut elapsed,
        );
        assert!(result.unwrap_err().contains("run exceeded"));
    }

    #[test]
    fn total_run_deadline_is_enforced_before_oracle_launch() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 1,
            total_timeout_ms: 1,
            oracle: Oracle::None,
            out_dir: root.path().to_path_buf(),
            ..RunOptions::default()
        };
        let mut calls = 0;
        let mut elapsed = || {
            calls += 1;
            if calls == 1 {
                Duration::ZERO
            } else {
                Duration::MAX
            }
        };
        let result = generate_run_with_elapsed(
            &options,
            Path::new("missing-compiler"),
            "test",
            &mut elapsed,
        );
        assert!(result.unwrap_err().contains("before compiler oracle"));
    }

    #[cfg(unix)]
    #[test]
    fn checked_cases_share_bounded_compiler_batches() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temporary fuzz root");
        let launches = root.path().join("launches");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            format!("#!/bin/sh\nprintf 'launch\\n' >> {}\n", launches.display()),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        let options = RunOptions {
            cases: 65,
            oracle: Oracle::Check,
            out_dir: root.path().join("cases"),
            case_timeout_ms: 1_000,
            total_timeout_ms: 10_000,
            rss_limit_bytes: 1024 * 1024 * 1024,
            cpu_limit_ms: 1_000,
            ..RunOptions::default()
        };
        generate_run(&options, &compiler, "test").expect("batched run");
        assert_eq!(
            fs::read_to_string(launches)
                .expect("launch ledger")
                .lines()
                .count(),
            3
        );
    }

    #[test]
    fn batch_artifact_failure_is_attributed() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::None,
            out_dir: root.path().to_path_buf(),
            ..RunOptions::default()
        };
        let case = generate(0, &options, "test");
        let directory = options.out_dir.join(&case.manifest.case_id);
        write_case(&directory, &case).expect("case artifacts");
        fs::create_dir(temporary_path(&directory.join("observed.json")))
            .expect("observation blocker");
        let mut elapsed = || Duration::ZERO;
        let error = run_batches(
            &options,
            Path::new("missing-compiler"),
            &[(directory, case)],
            &mut elapsed,
        )
        .expect_err("artifact publish must fail");
        assert!(error.contains("write temporary"));
    }

    #[cfg(unix)]
    #[test]
    fn checked_artifacts_record_success_and_reject_failed_oracles() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::Check,
            out_dir: root.path().join("passing"),
            ..RunOptions::default()
        };
        generate_run(&options, Path::new("/bin/true"), "test").expect("passing oracle");
        let case = generate(0, &options, "test");
        let directory = options.out_dir.join(&case.manifest.case_id);
        replay(&directory, Path::new("/bin/true")).expect("checked replay");

        let failed = run_and_record(&directory, &case, Path::new("/bin/false"))
            .expect_err("failed compiler must fail the case");
        assert!(failed.contains("ended as failed"));
        assert!(fs::read_to_string(directory.join("resources.json"))
            .expect("resources")
            .contains("process_group_cleanup"));

        assert!(replay(&directory, Path::new("/bin/false"))
            .unwrap_err()
            .contains("ended as failed"));
    }

    #[test]
    fn filesystem_and_serialization_failures_are_attributed() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let blocked = root.path().join("blocked");
        fs::write(&blocked, "file").expect("blocking file");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::None,
            out_dir: blocked.clone(),
            ..RunOptions::default()
        };
        assert!(generate_run(&options, Path::new("missing"), "test")
            .unwrap_err()
            .contains("create fuzz output"));

        let output = root.path().join("cases");
        fs::create_dir(&output).expect("cases");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::None,
            out_dir: output.clone(),
            ..RunOptions::default()
        };
        let case = generate(0, &options, "test");
        fs::write(output.join(&case.manifest.case_id), "blocking case")
            .expect("blocking case file");
        assert!(generate_run(&options, Path::new("missing"), "test")
            .unwrap_err()
            .contains("create case directory"));

        let missing = root.path().join("missing/entry");
        assert!(atomic_write(&missing, b"value")
            .unwrap_err()
            .contains("write temporary"));
        let destination = root.path().join("destination");
        fs::create_dir(&destination).expect("destination directory");
        assert!(atomic_write(&destination, b"value")
            .unwrap_err()
            .contains("publish fuzz artifact"));
    }

    #[cfg(unix)]
    #[test]
    fn every_case_artifact_publish_failure_is_attributed() {
        let case = generate(0, &test_options(5), "test");
        for name in [
            "Cargo.toml",
            CASE_SOURCE_FILE,
            "replay.terl",
            "case.toml",
            "expected.json",
            "timings.json",
        ] {
            let root = tempfile::tempdir().expect("temporary fuzz root");
            let blocker = temporary_path(&root.path().join(name));
            fs::create_dir(&blocker).expect("temporary path blocker");
            let error = write_case(root.path(), &case).expect_err("publish must fail");
            assert!(error.contains("write temporary"), "{name}: {error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn every_observation_artifact_publish_failure_is_attributed() {
        let mut options = test_options(5);
        options.oracle = Oracle::Check;
        let case = generate(0, &options, "test");
        for name in [
            "observed.json",
            "diagnostics.json",
            "timings.json",
            "resources.json",
        ] {
            let root = tempfile::tempdir().expect("temporary fuzz root");
            write_case(root.path(), &case).expect("case artifacts");
            let blocker = temporary_path(&root.path().join(name));
            fs::create_dir(&blocker).expect("temporary path blocker");
            let error = run_and_record(root.path(), &case, Path::new("/bin/true"))
                .expect_err("publish must fail");
            assert!(error.contains("write temporary"), "{name}: {error}");
        }

        let root = tempfile::tempdir().expect("temporary fuzz root");
        write_case(root.path(), &case).expect("case artifacts");
        let error = run_and_record(root.path(), &case, Path::new("missing-compiler"))
            .expect_err("missing compiler must fail");
        assert!(error.contains("infrastructure_error"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn run_and_replay_attribute_nested_failures() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::Check,
            out_dir: root.path().join("failing"),
            ..RunOptions::default()
        };
        assert!(generate_run(&options, Path::new("/bin/false"), "test")
            .unwrap_err()
            .contains("ended as failed"));

        let case = generate(0, &test_options(5), "test");
        write_case(root.path(), &case).expect("case artifacts");
        fs::remove_file(root.path().join(CASE_SOURCE_FILE)).expect("remove replay source");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("read replay artifact"));
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
