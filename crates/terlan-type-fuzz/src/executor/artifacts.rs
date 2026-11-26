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
    Err(format!(
        "{subject} {} oracle ended as {}",
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
mod tests {
    use super::*;

    #[test]
    fn every_oracle_has_stable_artifact_spelling() {
        assert_eq!(oracle_name(Oracle::Check), "check");
        assert_eq!(oracle_name(Oracle::JavaScript), "js");
        assert_eq!(oracle_name(Oracle::Native), "native");
        assert_eq!(oracle_name(Oracle::None), "none");
    }

    #[test]
    fn batch_failures_name_implicated_cases_or_the_exact_batch() {
        let options = test_options(5);
        let batch = [0, 17]
            .into_iter()
            .map(|index| {
                (
                    PathBuf::from(index.to_string()),
                    generate(index, &options, "test"),
                )
            })
            .collect::<Vec<_>>();
        let result = |outcome: &'static str, stderr: &str| OracleResult {
            schema: "terlan.type-fuzz-observation.v1",
            oracle: Oracle::None,
            outcome,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: stderr.into(),
            resources: ResourceEvidence::unmeasured(1, 1, 1),
        };
        assert_eq!(ensure_batch_success(&batch, &result("passed", "")), Ok(()));
        let implicated = ensure_batch_success(
            &batch,
            &result(
                "failed",
                "would reformat src/type_fuzz/case000017/Batch.terl",
            ),
        )
        .expect_err("batch must fail");
        assert!(implicated.contains(&batch[1].1.manifest.case_id));
        assert!(!implicated.contains(&batch[0].1.manifest.case_id));

        let unknown = ensure_batch_success(&batch, &result("failed", "unclassified"))
            .expect_err("batch must fail");
        assert!(unknown.contains(&format!(
            "batch {}..{}",
            batch[0].1.manifest.case_id, batch[1].1.manifest.case_id
        )));
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
        assert!(root.path().join(PROJECT_MANIFEST_FILE).is_file());
        assert!(!root.path().join("Cargo.toml").exists());
        assert!(root.path().join("src").is_dir());
        assert!(root.path().join(PROJECT_SOURCE_ROOT_MARKER).is_file());
        let first = fs::read(root.path().join("case.toml")).expect("manifest");
        write_case(root.path(), &case).expect("second write");
        assert_eq!(
            first,
            fs::read(root.path().join("case.toml")).expect("manifest")
        );
        verify_case(root.path(), &case).expect("replay identity");
    }

    #[test]
    fn multi_module_artifacts_are_written_and_verified() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let case = generate(17, &test_options(5), "test");
        assert_eq!(case.support_sources.len(), 4);
        write_case(root.path(), &case).expect("multi-module artifacts");
        verify_case(root.path(), &case).expect("multi-module replay identity");
        for support in &case.support_sources {
            assert_eq!(
                fs::read_to_string(root.path().join(&support.path)).expect("support source"),
                support.source
            );
        }

        let changed = &case.support_sources[0];
        fs::write(root.path().join(&changed.path), "changed").expect("change support source");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("drifted"));
        fs::remove_file(root.path().join(&changed.path)).expect("remove support source");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("read replay support source"));
    }

    #[test]
    fn multi_module_offline_replay_collects_every_source() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let mut options = test_options(5);
        options.oracle = Oracle::None;
        let case = generate(17, &options, "test");
        write_case(root.path(), &case).expect("multi-module artifacts");
        run_and_record(root.path(), &case, Path::new("missing-compiler"))
            .expect("offline multi-module replay");
        assert!(fs::read_to_string(root.path().join("observed.json"))
            .expect("observation")
            .contains("not_run"));
    }

    #[cfg(unix)]
    #[test]
    fn checked_multi_module_replay_compares_interface_facts() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let mut options = test_options(5);
        options.oracle = Oracle::Check;
        options.case_timeout_ms = 1_000;
        options.cpu_limit_ms = 1_000;
        options.rss_limit_bytes = 1 << 30;
        let case = generate(17, &options, "test");
        write_case(root.path(), &case).expect("multi-module artifacts");
        let cache = root.path().join(".terlan");
        fs::create_dir(&cache).expect("interface cache");
        for support in &case.support_sources {
            let module = interface_roundtrip::support_modules(std::slice::from_ref(support))
                .expect("support module")
                .remove(0);
            fs::write(cache.join(format!("{module}.typi")), &support.source)
                .expect("support interface");
        }
        run_and_record(root.path(), &case, Path::new("/bin/true"))
            .expect("source and interface consumers");
        assert!(
            fs::read_to_string(root.path().join("interface-roundtrip.json"))
                .expect("interface evidence")
                .contains("source_and_interface_consumers_accepted")
        );
        let module = interface_roundtrip::support_modules(&case.support_sources)
            .expect("support modules")
            .remove(0);
        fs::remove_file(cache.join(format!("{module}.typi"))).expect("remove interface");
        assert!(run_and_record(root.path(), &case, Path::new("/bin/true"))
            .unwrap_err()
            .contains("copy interface round-trip input"));
    }

    #[cfg(unix)]
    #[test]
    fn batched_interface_comparison_honors_deadlines_and_failures() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::Check,
            out_dir: root.path().join("cases"),
            case_timeout_ms: 1_000,
            total_timeout_ms: 1_000,
            rss_limit_bytes: 1 << 30,
            cpu_limit_ms: 1_000,
            ..RunOptions::default()
        };
        fs::create_dir(&options.out_dir).expect("cases root");
        let case = generate(17, &options, "test");
        let directory = options.out_dir.join(&case.manifest.case_id);
        write_case(&directory, &case).expect("case artifacts");
        let mut calls = 0;
        let mut elapsed = || {
            calls += 1;
            if calls == 1 {
                Duration::ZERO
            } else {
                Duration::from_millis(1_000)
            }
        };
        assert!(run_batches(
            &options,
            Path::new("/bin/true"),
            &[(directory.clone(), case.clone())],
            &mut elapsed,
        )
        .unwrap_err()
        .contains("before interface comparison"));

        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            concat!(
                "#!/bin/sh\n",
                "case \"$PWD\" in\n",
                "  *.interface-roundtrip-*) exit 1 ;;\n",
                "esac\n",
                "find . -path '*/src/*' -name '*.terl' -type f | while read -r source; do\n",
                "  module=$(sed -n 's/^module \\([^ ]*\\)\\.$/\\1/p' \"$source\")\n",
                "  [ -n \"$module\" ] || continue\n",
                "  project=${source%%/src/*}\n",
                "  mkdir -p \"$project/.terlan\"\n",
                "  cp \"$source\" \"$project/.terlan/$module.typi\"\n",
                "done\n",
            ),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        let mut elapsed = || Duration::ZERO;
        assert!(
            run_batches(&options, &compiler, &[(directory, case)], &mut elapsed,)
                .unwrap_err()
                .contains("interface-only semantic comparison ended as failed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn batched_package_comparison_honors_deadlines_and_failures() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temporary fuzz root");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::Check,
            out_dir: root.path().join("cases"),
            case_timeout_ms: 1_000,
            total_timeout_ms: 1_000,
            rss_limit_bytes: 1 << 30,
            cpu_limit_ms: 1_000,
            ..RunOptions::default()
        };
        fs::create_dir(&options.out_dir).expect("cases root");
        let case = generate(17, &options, "test");
        let directory = options.out_dir.join(&case.manifest.case_id);
        write_case(&directory, &case).expect("case artifacts");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            concat!(
                "#!/bin/sh\n",
                "find . -path '*/src/*' -name '*.terl' -type f | while read -r source; do\n",
                "  module=$(sed -n 's/^module \\([^ ]*\\)\\.$/\\1/p' \"$source\")\n",
                "  [ -n \"$module\" ] || continue\n",
                "  project=${source%%/src/*}\n",
                "  mkdir -p \"$project/.terlan\"\n",
                "  cp \"$source\" \"$project/.terlan/$module.typi\"\n",
                "done\n",
                "case \"$PWD\" in\n",
                "  *.package-roundtrip-*/consumer) exit 1 ;;\n",
                "esac\n",
            ),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");

        let mut calls = 0;
        let mut elapsed = || {
            calls += 1;
            if calls < 3 {
                Duration::ZERO
            } else {
                Duration::from_millis(1_000)
            }
        };
        assert!(run_batches(
            &options,
            &compiler,
            &[(directory.clone(), case.clone())],
            &mut elapsed,
        )
        .unwrap_err()
        .contains("before package comparison"));

        let mut elapsed = || Duration::ZERO;
        assert!(
            run_batches(&options, &compiler, &[(directory, case)], &mut elapsed)
                .unwrap_err()
                .contains("path-dependency semantic comparison ended as failed")
        );
    }

    #[test]
    fn support_source_paths_are_confined_and_unique() {
        let replay_root = tempfile::tempdir().expect("temporary fuzz root");
        let replay_case = generate(0, &test_options(5), "test");
        write_case(replay_root.path(), &replay_case).expect("valid replay artifacts");
        let invalid = [
            "",
            "Support.txt",
            "../Support.terl",
            "/Support.terl",
            "./Support.terl",
            "Support.terl",
            CASE_SOURCE_FILE,
            "replay.terl",
        ];
        for path in invalid {
            let mut case = generate(0, &test_options(5), "test");
            case.support_sources = vec![GeneratedSource {
                path: PathBuf::from(path),
                source: "module Support.".into(),
            }];
            assert!(write_case(Path::new("unused"), &case)
                .unwrap_err()
                .contains("invalid generated support source path"));
            assert!(verify_case(replay_root.path(), &case)
                .unwrap_err()
                .contains("invalid generated support source path"));
        }

        let mut duplicate = generate(0, &test_options(5), "test");
        let support = GeneratedSource {
            path: PathBuf::from("src/Support.terl"),
            source: "module Support.".into(),
        };
        duplicate.support_sources = vec![support.clone(), support];
        assert!(write_case(Path::new("unused"), &duplicate)
            .unwrap_err()
            .contains("duplicate generated support source path"));

        let mut nested_below_source = generate(0, &test_options(5), "test");
        nested_below_source.support_sources = vec![
            GeneratedSource {
                path: PathBuf::from("src/Support.terl"),
                source: "module Support.".into(),
            },
            GeneratedSource {
                path: PathBuf::from("src/Support.terl/Nested.terl"),
                source: "module Nested.".into(),
            },
        ];
        assert!(write_case(Path::new("unused"), &nested_below_source)
            .unwrap_err()
            .contains("contains source parent"));

        let mut invalid_module = generate(0, &test_options(5), "test");
        invalid_module.support_sources = vec![GeneratedSource {
            path: PathBuf::from("src/Invalid.terl"),
            source: "not a module".into(),
        }];
        assert!(write_case(Path::new("unused"), &invalid_module)
            .unwrap_err()
            .contains("has no valid module declaration"));
    }

    #[test]
    fn source_root_support_source_is_replayable() {
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let mut case = generate(0, &test_options(5), "test");
        case.support_sources = vec![GeneratedSource {
            path: PathBuf::from("src/Support.terl"),
            source: "module Support.\n".into(),
        }];
        write_case(root.path(), &case).expect("root support source");
        verify_case(root.path(), &case).expect("root support replay identity");
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

        write_case(root.path(), &case).expect("restore supported manifest");
        let manifest = fs::read_to_string(root.path().join("case.toml"))
            .expect("manifest")
            .replace(
                "terlan.type-fuzz-generator.v6",
                "terlan.type-fuzz-generator.v7",
            );
        fs::write(root.path().join("case.toml"), manifest).expect("future generator");
        assert!(replay(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("unsupported type fuzz generator"));

        fs::write(root.path().join("case.toml"), "not valid toml = [").expect("invalid manifest");
        assert!(replay(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("invalid"));
    }

    #[test]
    fn corpus_check_replays_sorted_cases_and_rejects_invalid_roots() {
        let root = tempfile::tempdir().expect("temporary corpus root");
        assert!(check_corpus(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("contains no replayable cases"));
        fs::write(root.path().join("README.md"), "corpus").expect("corpus readme");
        fs::create_dir(root.path().join("ignored")).expect("ignored directory");

        let options = RunOptions {
            oracle: Oracle::None,
            ..RunOptions::default()
        };
        let first = generate(0, &options, "test");
        let second = generate(1, &options, "test");
        write_case(&root.path().join("z-case"), &second).expect("second case");
        write_case(&root.path().join("a-case"), &first).expect("first case");
        check_corpus(root.path(), Path::new("missing-compiler")).expect("valid corpus");

        fs::write(root.path().join("z-case").join(CASE_SOURCE_FILE), "drifted")
            .expect("corrupt case");
        assert!(check_corpus(root.path(), Path::new("missing-compiler"))
            .unwrap_err()
            .contains("drifted"));

        let file = root.path().join("not-a-directory");
        fs::write(&file, "file").expect("corpus file");
        assert!(check_corpus(&file, Path::new("missing-compiler"))
            .unwrap_err()
            .contains("read corpus directory"));
        let failed_entry: std::io::Result<fs::DirEntry> =
            Err(std::io::Error::other("entry failure"));
        let mut failed_entries = [failed_entry].into_iter();
        assert!(check_corpus_entries(
            root.path(),
            Path::new("missing-compiler"),
            &mut failed_entries,
        )
        .unwrap_err()
        .contains("read corpus entry"));
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
            PROJECT_MANIFEST_FILE,
            PROJECT_SOURCE_ROOT_MARKER,
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

    #[test]
    fn total_run_deadline_is_enforced_before_planning_probe() {
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
            if calls < 3 {
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
        assert!(result.unwrap_err().contains("before planning oracle"));
    }

    #[cfg(unix)]
    #[test]
    fn planning_probe_failure_is_attributed_by_the_run() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temporary fuzz root");
        let launches = root.path().join("launches");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            format!(
                "#!/bin/sh\nprintf 'launch\\n' >> {}\n[ $(wc -l < {}) -lt 3 ]\n",
                launches.display(),
                launches.display(),
            ),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        let options = RunOptions {
            cases: 1,
            oracle: Oracle::Native,
            out_dir: root.path().join("cases"),
            case_timeout_ms: 1_000,
            total_timeout_ms: 5_000,
            ..RunOptions::default()
        };
        assert!(generate_run(&options, &compiler, "test")
            .unwrap_err()
            .contains("narrow cold native planning probe"));
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
            format!(
                concat!(
                    "#!/bin/sh\n",
                    "printf 'launch\\n' >> {}\n",
                    "grep -R -q 'pub broken(' . && exit 1\n",
                    "find . -path '*/src/*' -name '*.terl' -type f | while read -r source; do\n",
                    "  module=$(sed -n 's/^module \\([^ ]*\\)\\.$/\\1/p' \"$source\")\n",
                    "  [ -n \"$module\" ] || continue\n",
                    "  project=${{source%%/src/*}}\n",
                    "  mkdir -p \"$project/.terlan\"\n",
                    "  cp \"$source\" \"$project/.terlan/$module.typi\"\n",
                    "done\n",
                ),
                launches.display()
            ),
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
            34
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
            PROJECT_MANIFEST_FILE,
            PROJECT_SOURCE_ROOT_MARKER,
            CASE_SOURCE_FILE,
            "replay.terl",
            "case.toml",
            "expected.json",
            "timings.json",
        ] {
            let root = tempfile::tempdir().expect("temporary fuzz root");
            let blocker = temporary_path(&root.path().join(name));
            fs::create_dir_all(blocker.parent().expect("artifact parent"))
                .expect("artifact parent");
            fs::create_dir(&blocker).expect("temporary path blocker");
            let error = write_case(root.path(), &case).expect_err("publish must fail");
            assert!(error.contains("write temporary"), "{name}: {error}");
        }

        let multi_module = generate(17, &test_options(5), "test");
        let root = tempfile::tempdir().expect("temporary fuzz root");
        let support = root.path().join(&multi_module.support_sources[0].path);
        fs::create_dir_all(support.parent().expect("support parent")).expect("support parent");
        fs::create_dir(temporary_path(&support)).expect("support blocker");
        assert!(write_case(root.path(), &multi_module)
            .unwrap_err()
            .contains("write temporary"));

        let root = tempfile::tempdir().expect("temporary fuzz root");
        fs::write(root.path().join("src"), "source root blocker").expect("source root blocker");
        assert!(write_case(root.path(), &multi_module)
            .unwrap_err()
            .contains("create case source root"));

        let root = tempfile::tempdir().expect("temporary fuzz root");
        fs::create_dir(root.path().join("src")).expect("source root");
        fs::write(root.path().join("src/type_fuzz"), "directory blocker")
            .expect("support directory blocker");
        assert!(write_case(root.path(), &multi_module)
            .unwrap_err()
            .contains("create support source directory"));
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

    #[cfg(unix)]
    #[test]
    fn native_fixtures_are_confined_executable_and_replay_checked() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temporary fuzz root");
        let case = generate(20, &test_options(5), "test");
        write_case(root.path(), &case).expect("native fixture artifacts");
        verify_case(root.path(), &case).expect("native fixture replay");
        let fixture = case.fixture.as_ref().expect("fixture");
        let helper = fixture
            .files
            .iter()
            .find(|file| file.executable)
            .expect("helper");
        let helper_path = root.path().join(&helper.path);
        assert_ne!(
            fs::metadata(&helper_path)
                .expect("helper metadata")
                .permissions()
                .mode()
                & 0o111,
            0
        );
        fs::write(&helper_path, "drift").expect("helper drift");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("replay input drifted"));

        write_case(root.path(), &case).expect("restore helper");
        fs::set_permissions(&helper_path, fs::Permissions::from_mode(0o600)).expect("mode drift");
        assert!(verify_case(root.path(), &case)
            .unwrap_err()
            .contains("mode drifted"));
    }

    #[test]
    fn native_fixture_validation_rejects_every_unsafe_binding() {
        let case = generate(20, &test_options(5), "test");
        let fixture = case.fixture.as_ref().expect("fixture");
        let support = GeneratedSource {
            path: PathBuf::from("src/Support.terl"),
            source: "module Support.\n".into(),
        };
        validate_fixture(Some(fixture), std::slice::from_ref(&support))
            .expect("distinct support path");
        let colliding_support = GeneratedSource {
            path: fixture.files[0].path.clone(),
            source: "module Collision.\n".into(),
        };
        assert!(validate_fixture(Some(fixture), &[colliding_support])
            .unwrap_err()
            .contains("invalid or duplicate"));
        let mut invalid_path = fixture.clone();
        invalid_path.files[0].path = PathBuf::from("../escape");
        assert!(validate_fixture(Some(&invalid_path), &[])
            .unwrap_err()
            .contains("invalid or duplicate"));

        let mut reserved = fixture.clone();
        reserved.files[0].path = PathBuf::from(CASE_SOURCE_FILE);
        assert!(validate_fixture(Some(&reserved), &[])
            .unwrap_err()
            .contains("invalid or duplicate"));

        let mut duplicate_path = fixture.clone();
        duplicate_path.files[1].path = duplicate_path.files[0].path.clone();
        assert!(validate_fixture(Some(&duplicate_path), &[])
            .unwrap_err()
            .contains("invalid or duplicate"));

        let mut invalid_name = fixture.clone();
        invalid_name.environment[0].name = "lowercase".into();
        assert!(validate_fixture(Some(&invalid_name), &[])
            .unwrap_err()
            .contains("invalid generated fixture environment"));

        let mut duplicate_name = fixture.clone();
        duplicate_name.environment[1].name = duplicate_name.environment[0].name.clone();
        assert!(validate_fixture(Some(&duplicate_name), &[])
            .unwrap_err()
            .contains("invalid generated fixture environment"));

        let mut non_executable = fixture.clone();
        non_executable.environment[0].executable = non_executable.files[0].path.clone();
        assert!(validate_fixture(Some(&non_executable), &[])
            .unwrap_err()
            .contains("invalid generated fixture environment"));
    }

    #[cfg(unix)]
    #[test]
    fn native_fixture_filesystem_failures_are_attributed() {
        let case = generate(20, &test_options(5), "test");
        let fixture = case.fixture.as_ref().expect("fixture");

        let root = tempfile::tempdir().expect("invalid fixture root");
        let mut invalid = case.clone();
        invalid.fixture.as_mut().expect("fixture").files[0].path = PathBuf::from("../escape");
        assert!(write_case(root.path(), &invalid)
            .unwrap_err()
            .contains("invalid or duplicate"));

        let root = tempfile::tempdir().expect("mkdir fixture root");
        fs::write(root.path().join("providers"), "blocker").expect("directory blocker");
        assert!(write_fixture(root.path(), fixture)
            .unwrap_err()
            .contains("create fixture directory"));

        let root = tempfile::tempdir().expect("write fixture root");
        let first = root.path().join(&fixture.files[0].path);
        fs::create_dir_all(first.parent().expect("fixture parent")).expect("fixture parent");
        fs::create_dir(temporary_path(&first)).expect("temporary blocker");
        assert!(write_fixture(root.path(), fixture)
            .unwrap_err()
            .contains("write temporary fuzz artifact"));

        let root = tempfile::tempdir().expect("write-case fixture root");
        let first = root.path().join(&fixture.files[0].path);
        fs::create_dir_all(first.parent().expect("fixture parent")).expect("fixture parent");
        fs::create_dir(temporary_path(&first)).expect("temporary blocker");
        assert!(write_case(root.path(), &case)
            .unwrap_err()
            .contains("write temporary fuzz artifact"));

        let root = tempfile::tempdir().expect("permission fixture root");
        let mut reject_permissions = |_: &Path, _: bool| Err("permission fixture".into());
        assert_eq!(
            write_fixture_with(root.path(), fixture, &mut reject_permissions),
            Err("permission fixture".into())
        );

        let root = tempfile::tempdir().expect("read fixture root");
        write_fixture(root.path(), fixture).expect("fixture files");
        fs::remove_file(root.path().join(&fixture.files[0].path)).expect("remove fixture file");
        assert!(verify_fixture(root.path(), fixture)
            .unwrap_err()
            .contains("read replay fixture"));

        let root = tempfile::tempdir().expect("metadata fixture root");
        write_fixture(root.path(), fixture).expect("fixture files");
        let mut reject_metadata = |_: &Path, _: bool| Err("metadata fixture".into());
        assert_eq!(
            verify_fixture_with(root.path(), fixture, &mut reject_metadata),
            Err("metadata fixture".into())
        );
        assert!(
            verify_executable(&root.path().join("missing-helper"), false)
                .unwrap_err()
                .contains("read fixture permissions")
        );

        let root = tempfile::tempdir().expect("environment fixture root");
        write_case(root.path(), &case).expect("case fixture");
        let helper = &fixture.environment[0].executable;
        fs::remove_file(root.path().join(helper)).expect("remove helper");
        assert!(
            run_and_record(root.path(), &case, Path::new("unused-compiler"))
                .unwrap_err()
                .contains("resolve fixture executable")
        );

        let root = tempfile::tempdir().expect("verify fixture root");
        write_case(root.path(), &case).expect("case fixture");
        let mut invalid = case.clone();
        invalid.fixture.as_mut().expect("fixture").files[0].path = PathBuf::from("../escape");
        assert!(verify_case(root.path(), &invalid)
            .unwrap_err()
            .contains("invalid or duplicate"));
    }

    #[cfg(unix)]
    #[test]
    fn batch_fixture_filesystem_failures_are_attributed() {
        let root = tempfile::tempdir().expect("batch root");
        let mut options = test_options(5);
        options.oracle = Oracle::Check;
        options.out_dir = root.path().join("cases");
        let case = generate(20, &options, "test");
        let directory = options.out_dir.join(&case.manifest.case_id);
        write_case(&directory, &case).expect("case fixture");
        let helper = case.fixture.as_ref().expect("fixture").environment[0]
            .executable
            .clone();
        fs::remove_file(directory.join(helper)).expect("remove helper");
        let mut elapsed = || Duration::ZERO;
        assert!(run_batches(
            &options,
            Path::new("unused-compiler"),
            &[(directory, case)],
            &mut elapsed,
        )
        .unwrap_err()
        .contains("resolve fixture executable"));

        for directory_failure in [0, 1] {
            let first_root = tempfile::tempdir().expect("first fixture root");
            let second_root = tempfile::tempdir().expect("second fixture root");
            let first = generate(20, &test_options(5), "test");
            let second = first.clone();
            write_case(first_root.path(), &first).expect("first fixture");
            write_case(second_root.path(), &second).expect("second fixture");
            let helper = &first.fixture.as_ref().expect("fixture").environment[0].executable;
            let failed = if directory_failure == 0 {
                first_root.path().join(helper)
            } else {
                second_root.path().join(helper)
            };
            fs::remove_file(&failed).expect("remove helper");
            fs::create_dir(&failed).expect("directory helper");
            let error = batch_environment(&[
                (first_root.path().to_path_buf(), first),
                (second_root.path().to_path_buf(), second),
            ])
            .expect_err("directory helper must fail");
            assert!(error.contains("read batch helper"));
        }
    }

    #[test]
    fn batch_helpers_must_have_identical_implementations() {
        let first_root = tempfile::tempdir().expect("first fuzz root");
        let second_root = tempfile::tempdir().expect("second fuzz root");
        let first = generate(20, &test_options(5), "test");
        let second = first.clone();
        write_case(first_root.path(), &first).expect("first fixture");
        write_case(second_root.path(), &second).expect("second fixture");
        assert_eq!(
            batch_environment(&[
                (first_root.path().to_path_buf(), first.clone()),
                (second_root.path().to_path_buf(), second.clone()),
            ])
            .expect("identical helper bindings")
            .len(),
            2
        );

        let mut second = second;
        second
            .fixture
            .as_mut()
            .expect("fixture")
            .files
            .iter_mut()
            .find(|file| file.executable)
            .expect("helper")
            .contents
            .push_str("# conflict\n");
        write_case(second_root.path(), &second).expect("second fixture");
        let error = batch_environment(&[
            (first_root.path().to_path_buf(), first),
            (second_root.path().to_path_buf(), second),
        ])
        .expect_err("conflicting helper must fail");
        assert!(error.contains("conflicting fuzz helper environment"));
    }

    #[test]
    fn native_fixture_cases_own_their_compiler_batch() {
        let options = test_options(5);
        let ordinary = generate(0, &options, "test");
        let native = generate(20, &options, "test");
        let pending = vec![
            (PathBuf::from("first"), ordinary.clone()),
            (PathBuf::from("native"), native),
            (PathBuf::from("last"), ordinary),
        ];
        assert_eq!(next_batch(&pending).len(), 1);
        assert_eq!(next_batch(&pending[1..]).len(), 1);
        assert_eq!(next_batch(&pending[2..]).len(), 1);

        let ordinary = generate(0, &options, "test");
        let many = (0..CHECK_BATCH_SIZE + 1)
            .map(|index| (PathBuf::from(index.to_string()), ordinary.clone()))
            .collect::<Vec<_>>();
        assert_eq!(next_batch(&many).len(), CHECK_BATCH_SIZE);
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
