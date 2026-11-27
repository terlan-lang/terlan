use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use super::interface_cache_transaction;
use super::interface_roundtrip;
use super::oracle::{self, OracleResult};
use super::package_roundtrip;
use super::planning;
use super::resources::ResourceEvidence;
use super::{generate_case as generate, regenerate_case};
use crate::{
    CaseManifest, GeneratedCase, GeneratedFixture, GeneratedSource, Oracle, RunOptions, TypedValue,
    CASE_SCHEMA,
};

const CHECK_BATCH_SIZE: usize = 32;
const CASE_SOURCE_FILE: &str = "CaseTest.terl";
const PROJECT_MANIFEST_FILE: &str = "terlan.toml";
const PROJECT_SOURCE_ROOT_MARKER: &str = "src/.keep";

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
    let spent_ms = u64::try_from(elapsed().as_millis()).unwrap_or(u64::MAX);
    let remaining_ms = options
        .total_timeout_ms
        .checked_sub(spent_ms)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(|| "run exceeded total timeout before planning oracle".to_string())?;
    planning::run(options, compiler, remaining_ms)?;
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
    let mut remaining = pending;
    while !remaining.is_empty() {
        let batch = next_batch(remaining);
        remaining = &remaining[batch.len()..];
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
        let formatting_sources = batch
            .iter()
            .flat_map(|(directory, case)| {
                let relative = directory
                    .strip_prefix(&options.out_dir)
                    .expect("case directory belongs to fuzz output");
                std::iter::once(relative.join(CASE_SOURCE_FILE))
                    .chain(
                        case.support_sources
                            .iter()
                            .map(|support| relative.join(&support.path)),
                    )
                    .chain(
                        fixture_sources(case)
                            .into_iter()
                            .map(|path| relative.join(path)),
                    )
            })
            .collect::<Vec<_>>();
        let environment = batch_environment(batch)?;
        let result = oracle::run_formatted_batch_inputs(
            compiler,
            options.oracle,
            oracle::FormattedBatchInputs {
                root: &options.out_dir,
                semantic_sources: &sources,
                formatting_sources: &formatting_sources,
                environment: &environment,
            },
            wall_ms,
            options.rss_limit_bytes,
            cpu_ms,
        );
        for (directory, case) in batch {
            record_observation(directory, &result, &case.manifest)?;
        }
        ensure_batch_success(batch, &result)?;
        for (directory, case) in batch {
            if case.support_sources.is_empty() || case.manifest.oracle == Oracle::None {
                continue;
            }
            let spent_ms = u64::try_from(elapsed().as_millis()).unwrap_or(u64::MAX);
            let remaining_ms = options
                .total_timeout_ms
                .checked_sub(spent_ms)
                .filter(|remaining| *remaining > 0)
                .ok_or_else(|| {
                    format!(
                        "run exceeded total timeout before interface comparison for {}",
                        case.manifest.case_id
                    )
                })?;
            interface_roundtrip::run(
                directory,
                case,
                compiler,
                remaining_ms.min(case.manifest.case_timeout_ms),
                case.manifest.rss_limit_bytes,
                case.manifest.cpu_limit_ms,
            )?;
            interface_cache_transaction::run(
                directory,
                case,
                compiler,
                remaining_ms.min(case.manifest.case_timeout_ms),
                case.manifest.rss_limit_bytes,
                case.manifest.cpu_limit_ms,
            )?;
            let spent_ms = u64::try_from(elapsed().as_millis()).unwrap_or(u64::MAX);
            let remaining_ms = options
                .total_timeout_ms
                .checked_sub(spent_ms)
                .filter(|remaining| *remaining > 0)
                .ok_or_else(|| {
                    format!(
                        "run exceeded total timeout before package comparison for {}",
                        case.manifest.case_id
                    )
                })?;
            package_roundtrip::run(
                directory,
                case,
                compiler,
                remaining_ms.min(case.manifest.case_timeout_ms),
                case.manifest.rss_limit_bytes,
                case.manifest.cpu_limit_ms,
            )?;
        }
    }
    Ok(())
}

fn next_batch(pending: &[(PathBuf, GeneratedCase)]) -> &[(PathBuf, GeneratedCase)] {
    let bounded = &pending[..pending.len().min(CHECK_BATCH_SIZE)];
    match bounded.iter().position(|(_, case)| case.fixture.is_some()) {
        Some(0) => &bounded[..1],
        Some(index) => &bounded[..index],
        None => bounded,
    }
}

pub(super) fn replay(directory: &Path, compiler: &Path) -> Result<(), String> {
    let case = load_generated_case(directory)?;
    run_and_record(directory, &case, compiler)?;
    println!("replayed {}", case.manifest.case_id);
    Ok(())
}

pub(super) fn load_generated_case(directory: &Path) -> Result<GeneratedCase, String> {
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
    let case = regenerate_case(
        manifest.case_index,
        &options,
        &manifest.compiler_revision,
        &manifest.generator,
        &manifest.family,
    )?;
    verify_case(directory, &case)?;
    Ok(case)
}

pub(super) fn check_corpus(root: &Path, compiler: &Path) -> Result<(), String> {
    let mut entries = fs::read_dir(root).map_err(io_error("read corpus directory"))?;
    check_corpus_entries(root, compiler, &mut entries)
}

fn check_corpus_entries(
    root: &Path,
    compiler: &Path,
    entries: &mut dyn Iterator<Item = std::io::Result<fs::DirEntry>>,
) -> Result<(), String> {
    let mut cases = Vec::new();
    for entry in entries {
        let path = entry
            .map(|entry| entry.path())
            .map_err(io_error("read corpus entry"))?;
        if path.is_dir() && path.join("case.toml").is_file() {
            cases.push(path);
        }
    }
    cases.sort();
    if cases.is_empty() {
        return Err(format!(
            "type fuzz corpus `{}` contains no replayable cases",
            root.display()
        ));
    }
    for case in &cases {
        replay(case, compiler)?;
    }
    println!("checked {} type fuzz corpus cases", cases.len());
    Ok(())
}

pub(super) fn write_case(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    validate_support_sources(&case.support_sources)?;
    validate_fixture(case.fixture.as_ref(), &case.support_sources)?;
    fs::create_dir_all(directory).map_err(io_error("create case directory"))?;
    fs::create_dir_all(directory.join("src")).map_err(io_error("create case source root"))?;
    atomic_write(&directory.join(PROJECT_SOURCE_ROOT_MARKER), b"")?;
    atomic_write(
        &directory.join(PROJECT_MANIFEST_FILE),
        case.project.as_bytes(),
    )?;
    atomic_write(&directory.join(CASE_SOURCE_FILE), case.source.as_bytes())?;
    atomic_write(&directory.join("replay.terl"), case.source.as_bytes())?;
    for support in &case.support_sources {
        let path = directory.join(&support.path);
        let parent = path
            .parent()
            .expect("validated support source belongs to the source root");
        fs::create_dir_all(parent).map_err(io_error("create support source directory"))?;
        atomic_write(&path, support.source.as_bytes())?;
    }
    if let Some(fixture) = &case.fixture {
        write_fixture(directory, fixture)?;
    }
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
    let semantic_sources = vec![PathBuf::from(CASE_SOURCE_FILE)];
    let formatting_sources = std::iter::once(PathBuf::from(CASE_SOURCE_FILE))
        .chain(
            case.support_sources
                .iter()
                .map(|support| support.path.clone()),
        )
        .chain(fixture_sources(case))
        .collect::<Vec<_>>();
    let environment = case_environment(directory, case)?;
    let result = oracle::run_formatted_batch_inputs(
        compiler,
        case.manifest.oracle,
        oracle::FormattedBatchInputs {
            root: directory,
            semantic_sources: &semantic_sources,
            formatting_sources: &formatting_sources,
            environment: &environment,
        },
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    );
    record_observation(directory, &result, &case.manifest)?;
    ensure_success(case, &result)?;
    if case.support_sources.is_empty() || case.manifest.oracle == Oracle::None {
        return Ok(());
    }
    interface_roundtrip::run(
        directory,
        case,
        compiler,
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    )?;
    interface_cache_transaction::run(
        directory,
        case,
        compiler,
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    )?;
    package_roundtrip::run(
        directory,
        case,
        compiler,
        case.manifest.case_timeout_ms,
        case.manifest.rss_limit_bytes,
        case.manifest.cpu_limit_ms,
    )
}

pub(super) fn record_observation(
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
    let resources = if matches!(result.oracle, Oracle::Native | Oracle::JavaScript)
        && result.outcome == "passed"
    {
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

fn ensure_batch_success(
    batch: &[(PathBuf, GeneratedCase)],
    result: &OracleResult,
) -> Result<(), String> {
    if matches!(result.outcome, "passed" | "not_run") {
        return Ok(());
    }
    let diagnostics = format!("{}\n{}", result.stdout, result.stderr)
        .lines()
        .filter(|line| !line.starts_with("terlc timing:") && !line.starts_with("terlc native-aot:"))
        .collect::<Vec<_>>()
        .join("\n");
    let implicated = batch
        .iter()
        .filter_map(|(_, case)| {
            let index = case.manifest.case_index;
            (diagnostics.contains(&case.manifest.case_id)
                || diagnostics.contains(&format!("Case{index:06}"))
                || diagnostics.contains(&format!("case{index:06}")))
            .then_some(case.manifest.case_id.as_str())
        })
        .collect::<Vec<_>>();
    let subject = if implicated.is_empty() {
        format!(
            "batch {}..{}",
            batch
                .first()
                .expect("non-empty fuzz batch")
                .1
                .manifest
                .case_id,
            batch
                .last()
                .expect("non-empty fuzz batch")
                .1
                .manifest
                .case_id,
        )
    } else {
        format!("case {}", implicated.join(", "))
    };
    let detail = if diagnostics.is_empty() {
        String::new()
    } else {
        format!(": {diagnostics}")
    };
    Err(format!(
        "{subject} {} oracle ended as {}{detail}",
        oracle_name(batch[0].1.manifest.oracle),
        result.outcome
    ))
}

fn verify_case(directory: &Path, case: &GeneratedCase) -> Result<(), String> {
    validate_support_sources(&case.support_sources)?;
    validate_fixture(case.fixture.as_ref(), &case.support_sources)?;
    let expected = [
        (PROJECT_MANIFEST_FILE, case.project.as_bytes().to_vec()),
        (PROJECT_SOURCE_ROOT_MARKER, Vec::new()),
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
    for support in &case.support_sources {
        let path = directory.join(&support.path);
        let actual = fs::read(&path).map_err(io_error("read replay support source"))?;
        if actual != support.source.as_bytes() {
            return Err(format!("replay input drifted: {}", path.display()));
        }
    }
    if let Some(fixture) = &case.fixture {
        verify_fixture(directory, fixture)?;
    }
    Ok(())
}

fn validate_support_sources(sources: &[GeneratedSource]) -> Result<(), String> {
    let mut paths = BTreeSet::new();
    for source in sources {
        let path = &source.path;
        let safe = !path.as_os_str().is_empty()
            && path.extension().and_then(|extension| extension.to_str()) == Some("terl")
            && path.starts_with("src")
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            && path != Path::new(CASE_SOURCE_FILE)
            && path != Path::new("replay.terl");
        if !safe {
            return Err(format!(
                "invalid generated support source path `{}`",
                path.display()
            ));
        }
        if !paths.insert(path.clone()) {
            return Err(format!(
                "duplicate generated support source path `{}`",
                path.display()
            ));
        }
    }
    for path in &paths {
        let mut parent = path.parent();
        while let Some(candidate) = parent {
            if paths.contains(candidate) {
                return Err(format!(
                    "generated support source path `{}` contains source parent `{}`",
                    path.display(),
                    candidate.display()
                ));
            }
            parent = candidate.parent();
        }
    }
    interface_roundtrip::support_modules(sources)?;
    Ok(())
}

fn fixture_sources(case: &GeneratedCase) -> Vec<PathBuf> {
    case.fixture
        .iter()
        .flat_map(|fixture| &fixture.files)
        .filter(|file| file.path.extension().and_then(|value| value.to_str()) == Some("terl"))
        .map(|file| file.path.clone())
        .collect()
}

fn validate_fixture(
    fixture: Option<&GeneratedFixture>,
    support_sources: &[GeneratedSource],
) -> Result<(), String> {
    let Some(fixture) = fixture else {
        return Ok(());
    };
    let mut paths = support_sources
        .iter()
        .map(|source| source.path.clone())
        .collect::<BTreeSet<_>>();
    let reserved = [
        Path::new(CASE_SOURCE_FILE),
        Path::new(PROJECT_MANIFEST_FILE),
        Path::new("replay.terl"),
        Path::new("case.toml"),
    ];
    for file in &fixture.files {
        let safe = !file.path.as_os_str().is_empty()
            && file
                .path
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            && !reserved.contains(&file.path.as_path());
        if !safe || !paths.insert(file.path.clone()) {
            return Err(format!(
                "invalid or duplicate generated fixture path `{}`",
                file.path.display()
            ));
        }
    }
    let executable = fixture
        .files
        .iter()
        .filter(|file| file.executable)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    for binding in &fixture.environment {
        let canonical_name = !binding.name.is_empty()
            && binding
                .name
                .chars()
                .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit() || value == '_');
        if !canonical_name
            || !names.insert(binding.name.clone())
            || !executable.contains(&binding.executable)
        {
            return Err(format!(
                "invalid generated fixture environment `{}` -> `{}`",
                binding.name,
                binding.executable.display()
            ));
        }
    }
    Ok(())
}

fn write_fixture(directory: &Path, fixture: &GeneratedFixture) -> Result<(), String> {
    write_fixture_with(directory, fixture, &mut set_executable)
}

fn write_fixture_with(
    directory: &Path,
    fixture: &GeneratedFixture,
    permissions: &mut dyn FnMut(&Path, bool) -> Result<(), String>,
) -> Result<(), String> {
    for file in &fixture.files {
        let path = directory.join(&file.path);
        let parent = path.parent().expect("validated fixture file has a parent");
        fs::create_dir_all(parent).map_err(io_error("create fixture directory"))?;
        atomic_write(&path, file.contents.as_bytes())?;
        permissions(&path, file.executable)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if executable { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(io_error("set fixture permissions"))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<(), String> {
    Ok(())
}

fn verify_fixture(directory: &Path, fixture: &GeneratedFixture) -> Result<(), String> {
    verify_fixture_with(directory, fixture, &mut verify_executable)
}

fn verify_fixture_with(
    directory: &Path,
    fixture: &GeneratedFixture,
    verify: &mut dyn FnMut(&Path, bool) -> Result<(), String>,
) -> Result<(), String> {
    for file in &fixture.files {
        let path = directory.join(&file.path);
        let actual = fs::read(&path).map_err(io_error("read replay fixture"))?;
        if actual != file.contents.as_bytes() {
            return Err(format!("replay input drifted: {}", path.display()));
        }
        verify(&path, file.executable)?;
    }
    Ok(())
}

#[cfg(unix)]
fn verify_executable(path: &Path, expected: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => return Err(io_error("read fixture permissions")(error)),
    };
    let actual = metadata.permissions().mode() & 0o111 != 0;
    if actual != expected {
        return Err(format!("replay fixture mode drifted: {}", path.display()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_executable(_path: &Path, _expected: bool) -> Result<(), String> {
    Ok(())
}

fn case_environment(
    directory: &Path,
    case: &GeneratedCase,
) -> Result<Vec<(String, PathBuf)>, String> {
    case.fixture
        .iter()
        .flat_map(|fixture| &fixture.environment)
        .map(|binding| {
            fs::canonicalize(directory.join(&binding.executable))
                .map(|path| (binding.name.clone(), path))
                .map_err(io_error("resolve fixture executable"))
        })
        .collect()
}

fn batch_environment(batch: &[(PathBuf, GeneratedCase)]) -> Result<Vec<(String, PathBuf)>, String> {
    let mut bindings = BTreeMap::<String, PathBuf>::new();
    for (directory, case) in batch {
        for (name, path) in case_environment(directory, case)? {
            if let Some(existing) = bindings.get(&name) {
                let same = fs::read(existing).map_err(io_error("read batch helper"))?
                    == fs::read(&path).map_err(io_error("read batch helper"))?;
                if !same {
                    return Err(format!("conflicting fuzz helper environment `{name}`"));
                }
                continue;
            }
            bindings.insert(name, path);
        }
    }
    Ok(bindings.into_iter().collect())
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
        Oracle::JavaScript => "js",
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

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
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
#[path = "artifacts_test.rs"]
mod tests;
