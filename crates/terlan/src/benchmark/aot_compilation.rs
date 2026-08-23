//! Comparable Terlan and Go AOT compilation benchmark recorder.

use std::env;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::hardware::{command_version, sha256, HardwareFingerprint};

mod model;
mod policy;
use model::{
    CompilationBenchmarkReport, CompilationCacheState, CompilationFixtureIdentity,
    CompilationMeasurement, CompilationTiming, CompilationToolchains,
};

/// Internal benchmark command that records compilation measurements.
pub(crate) const COMMAND: &str = "aot-compilation";
/// Internal benchmark command that validates report contracts without timing.
pub(crate) const SELF_TEST_COMMAND: &str = "aot-compilation-self-test";
/// Internal benchmark command that enforces the committed performance policy.
pub(crate) const VALIDATE_COMMAND: &str = "aot-compilation-validate";

const REPORT_SCHEMA: &str = "terlan-aot-compilation-benchmark-v1";
const DEFAULT_OUTPUT: &str = "../benchmarks/results/aot-compilation-baseline.latest.json";
const DEFAULT_SAMPLE_COUNT: usize = 7;

/// Resolved command paths and fixture controls for one recording.
struct CompilationBenchmarkOptions {
    /// Terlan compiler executable.
    terlc: PathBuf,
    /// Terlan VM executable used for correctness checks.
    terlan_vm: PathBuf,
    /// Go command executable.
    go: PathBuf,
    /// Committed equivalent fixture root.
    fixtures: PathBuf,
    /// Number of samples per row.
    sample_count: usize,
}

/// Source workload selected for a benchmark row.
#[derive(Clone, Copy)]
enum FixtureKind {
    /// One source file producing one command image.
    Small,
    /// Two-module package producing one application image.
    Multi,
}

/// Mutation applied after an untimed warm population build.
#[derive(Clone, Copy)]
enum WarmMutation {
    /// No source change before the timed no-op build.
    None,
    /// Change only the dependency package implementation.
    Dependency,
    /// Change only the root command package implementation.
    Root,
}

/// Prepared equivalent Terlan and Go sample workspace.
struct SampleWorkspace {
    /// Terlan source file or project directory passed to `terlc build`.
    terlan_input: PathBuf,
    /// Expected Terlan image after a successful build.
    terlan_image: PathBuf,
    /// Go module root.
    go_root: PathBuf,
    /// Go package selector passed to `go build`.
    go_package: &'static str,
    /// Go command output path.
    go_output: PathBuf,
}

/// Live Terlan REPL process used to isolate startup and generation timing.
struct ReplSession {
    /// Child process lifetime owner.
    child: Child,
    /// Writable REPL command stream.
    stdin: ChildStdin,
    /// Buffered REPL output stream.
    stdout: BufReader<ChildStdout>,
}

impl ReplSession {
    /// Starts one compiler service and returns time to the initial prompt.
    fn start(terlc: &Path, workspace: &Path) -> Result<(Self, Duration), String> {
        let started = Instant::now();
        let mut child = Command::new(terlc)
            .arg("repl")
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to start Terlan REPL: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Terlan REPL has no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Terlan REPL has no stdout".to_string())?;
        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        };
        session.read_prompt()?;
        Ok((session, started.elapsed()))
    }

    /// Evaluates one expression and returns declaration-to-next-prompt latency.
    fn evaluate(&mut self, expression: &str, expected: &str) -> Result<Duration, String> {
        let started = Instant::now();
        writeln!(self.stdin, "{expression}.")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("failed to write Terlan REPL expression: {error}"))?;
        let output = self.read_prompt()?;
        let elapsed = started.elapsed();
        if !output.contains(expected) {
            return Err(format!(
                "Terlan REPL expression `{expression}` did not render `{expected}`: {output:?}"
            ));
        }
        Ok(elapsed)
    }

    /// Stops the REPL cleanly and verifies its process exit status.
    fn stop(mut self) -> Result<(), String> {
        writeln!(self.stdin, ":quit")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("failed to stop Terlan REPL: {error}"))?;
        drop(self.stdin);
        let status = self
            .child
            .wait()
            .map_err(|error| format!("failed to wait for Terlan REPL: {error}"))?;
        if !status.success() {
            return Err(format!("Terlan REPL exited with {status}"));
        }
        Ok(())
    }

    /// Reads bytes through the next flushed `repl> ` prompt.
    fn read_prompt(&mut self) -> Result<String, String> {
        const PROMPT: &[u8] = b"repl> ";
        let mut bytes = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            let read = self
                .stdout
                .read(&mut byte)
                .map_err(|error| format!("failed to read Terlan REPL output: {error}"))?;
            if read == 0 {
                return Err("Terlan REPL closed before emitting its prompt".to_string());
            }
            bytes.push(byte[0]);
            if bytes.ends_with(PROMPT) {
                return String::from_utf8(bytes)
                    .map_err(|error| format!("Terlan REPL emitted invalid UTF-8: {error}"));
            }
        }
    }
}

/// Runs the compilation recorder and writes its machine-readable report.
pub(crate) fn run_cli() -> ExitCode {
    match record_from_environment() {
        Ok((path, report)) => {
            println!(
                "AOT compilation benchmark recorded {} measurements to {}",
                report.measurements.len(),
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("AOT compilation benchmark failed: {error}");
            ExitCode::from(1)
        }
    }
}

/// Runs production-compiled report and fixture contract validation.
pub(crate) fn run_self_test_cli() -> ExitCode {
    match self_test::run() {
        Ok(()) => {
            println!("AOT compilation benchmark self-test passed");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("AOT compilation benchmark self-test failed: {error}");
            ExitCode::from(1)
        }
    }
}

/// Validates a recorded report against the committed ratio and latency policy.
pub(crate) fn run_validate_cli() -> ExitCode {
    let report = env::var_os("TERLAN_BENCH_AOT_COMPILATION_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/quality/aot-compilation-benchmark.json"));
    let policy = env::var_os("TERLAN_BENCH_AOT_COMPILATION_POLICY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("benchmarks/baselines/aot-compilation-limits.json"));
    match policy::validate_files(&report, &policy) {
        Ok(()) => {
            println!("AOT compilation benchmark satisfies {}", policy.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("AOT compilation benchmark policy failed: {error}");
            ExitCode::from(1)
        }
    }
}

/// Resolves environment controls, records all scenarios, and publishes JSON.
fn record_from_environment() -> Result<(PathBuf, CompilationBenchmarkReport), String> {
    let options = CompilationBenchmarkOptions::from_environment()?;
    let output = env::var_os("TERLAN_BENCH_AOT_COMPILATION_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT));
    let report = record(&options)?;
    validate_report(&report)?;
    write_report(&output, &report)?;
    Ok((output, report))
}

impl CompilationBenchmarkOptions {
    /// Resolves benchmark commands and committed fixtures from the environment.
    fn from_environment() -> Result<Self, String> {
        let current = env::current_exe()
            .map_err(|error| format!("cannot resolve benchmark executable: {error}"))?;
        let bin_dir = current
            .parent()
            .ok_or_else(|| "benchmark executable has no parent directory".to_string())?;
        let terlc = env::var_os("TERLAN_BENCH_TERLC")
            .map(PathBuf::from)
            .unwrap_or_else(|| bin_dir.join(executable_name("terlc")));
        let terlan_vm = env::var_os("TERLAN_BENCH_TERLAN_VM")
            .map(PathBuf::from)
            .unwrap_or_else(|| bin_dir.join(executable_name("terlan-vm")));
        let go = env::var_os("TERLAN_BENCH_GO")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("go"));
        let fixtures = env::var_os("TERLAN_BENCH_AOT_FIXTURES")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join("benchmarks/fixtures/aot_compilation")
            });
        let sample_count = env::var("TERLAN_BENCH_AOT_SAMPLES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_SAMPLE_COUNT);
        for (label, path) in [("terlc", &terlc), ("terlan-vm", &terlan_vm)] {
            if !path.is_file() {
                return Err(format!(
                    "{label} executable `{}` is missing",
                    path.display()
                ));
            }
        }
        if !fixtures.is_dir() {
            return Err(format!(
                "AOT compilation fixture directory `{}` is missing",
                fixtures.display()
            ));
        }
        Ok(Self {
            terlc,
            terlan_vm,
            go,
            fixtures,
            sample_count,
        })
    }
}

/// Records every required cold, incremental, relink, and REPL scenario.
fn record(options: &CompilationBenchmarkOptions) -> Result<CompilationBenchmarkReport, String> {
    let workspace = create_workspace()?;
    let result = record_in_workspace(options, &workspace);
    let cleanup = fs::remove_dir_all(&workspace)
        .map_err(|error| format!("failed to remove benchmark workspace: {error}"));
    match (result, cleanup) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

/// Records all scenarios inside one isolated temporary workspace.
fn record_in_workspace(
    options: &CompilationBenchmarkOptions,
    workspace: &Path,
) -> Result<CompilationBenchmarkReport, String> {
    let mut measurements = vec![
        measure_cold_pair(options, workspace, FixtureKind::Small, false)?,
        measure_cold_pair(options, workspace, FixtureKind::Multi, false)?,
        measure_warm_pair(options, workspace, WarmMutation::Dependency)?,
        measure_warm_pair(options, workspace, WarmMutation::None)?,
        measure_cold_pair(options, workspace, FixtureKind::Multi, true)?,
        measure_warm_pair(options, workspace, WarmMutation::Root)?,
    ];
    measurements.extend(measure_repl(options, workspace)?);

    let canonical_terlc = fs::canonicalize(&options.terlc)
        .map_err(|error| format!("cannot canonicalize terlc: {error}"))?;
    Ok(CompilationBenchmarkReport {
        schema: REPORT_SCHEMA.to_string(),
        status: "completed".to_string(),
        recorded_unix_seconds: unix_timestamp_seconds(),
        hardware: HardwareFingerprint::current(),
        toolchains: CompilationToolchains {
            rustc: command_version("rustc", &["--version"]),
            go: command_version(options.go.to_string_lossy().as_ref(), &["version"]),
            terlc_path: canonical_terlc.display().to_string(),
            terlc_sha256: sha256_file(&canonical_terlc)?,
        },
        fixtures: CompilationFixtureIdentity {
            path: "benchmarks/fixtures/aot_compilation".to_string(),
            sha256: fixture_sha256(&options.fixtures)?,
            workloads: vec!["small-command".to_string(), "multi-package".to_string()],
        },
        sample_count: options.sample_count,
        cache_state: CompilationCacheState {
            terlan_cold: "fresh project output and compiler-owned native cache per sample"
                .to_string(),
            go_cold: "fresh project output with the host Go build cache retained; a per-sample source nonce forces command package compilation"
                .to_string(),
            warm: "one untimed successful build populates project caches before each measured edit or no-op build"
                .to_string(),
            dependency_downloads_timed: false,
        },
        measurements,
    })
}

/// Measures a cold small or multi-package build in both compiler lanes.
fn measure_cold_pair(
    options: &CompilationBenchmarkOptions,
    workspace: &Path,
    kind: FixtureKind,
    release: bool,
) -> Result<CompilationMeasurement, String> {
    let name = match (kind, release) {
        (FixtureKind::Small, false) => "small_cold_development",
        (FixtureKind::Multi, false) => "multi_cold_development",
        (FixtureKind::Multi, true) => "cold_release",
        (FixtureKind::Small, true) => "small_cold_release",
    };
    let mut terlan = Vec::with_capacity(options.sample_count);
    let mut go = Vec::with_capacity(options.sample_count);
    for index in 0..options.sample_count {
        let root = fresh_sample_directory(workspace, name, index)?;
        let sample = prepare_sample(options, &root, kind)?;
        add_sample_nonce(&sample, kind, index)?;
        terlan.push(run_terlan_build(options, &sample, release)?);
        verify_terlan_image(options, &sample, kind)?;
        go.push(run_go_build(options, &sample, release)?);
        verify_go_image(&sample.go_output)?;
    }
    comparable_measurement(
        name,
        if release {
            "fresh multi-package output/cache; optimized whole-application Terlan build and optimized stripped Go command build"
        } else {
            "fresh output/cache and complete command build; fixture copying and correctness execution excluded"
        },
        terlan,
        go,
    )
}

/// Measures one dependency edit, no-op, or root-package relink in both lanes.
fn measure_warm_pair(
    options: &CompilationBenchmarkOptions,
    workspace: &Path,
    mutation: WarmMutation,
) -> Result<CompilationMeasurement, String> {
    let (name, scope) = match mutation {
        WarmMutation::Dependency => (
            "one_package_edit",
            "dependency implementation edit through incremental analysis, object regeneration, and final application link",
        ),
        WarmMutation::None => (
            "no_op_development",
            "unchanged multi-package incremental command after one untimed cache-population build",
        ),
        WarmMutation::Root => (
            "package_relink",
            "root command implementation edit through one package object regeneration and final application link",
        ),
    };
    let mut terlan = Vec::with_capacity(options.sample_count);
    let mut go = Vec::with_capacity(options.sample_count);
    for index in 0..options.sample_count {
        let root = fresh_sample_directory(workspace, name, index)?;
        let sample = prepare_sample(options, &root, FixtureKind::Multi)?;
        add_sample_nonce(&sample, FixtureKind::Multi, index)?;
        run_terlan_build(options, &sample, false)?;
        run_go_build(options, &sample, false)?;
        apply_warm_mutation(&sample, mutation)?;
        terlan.push(run_terlan_build(options, &sample, false)?);
        go.push(run_go_build(options, &sample, false)?);
    }
    comparable_measurement(name, scope, terlan, go)
}

/// Measures compiler-service startup and first, changed, and unchanged REPL work.
fn measure_repl(
    options: &CompilationBenchmarkOptions,
    workspace: &Path,
) -> Result<Vec<CompilationMeasurement>, String> {
    let mut startup = Vec::with_capacity(options.sample_count);
    let mut first = Vec::with_capacity(options.sample_count);
    for index in 0..options.sample_count {
        let root = fresh_sample_directory(workspace, "first_repl", index)?;
        let (mut session, started) = ReplSession::start(&options.terlc, &root)?;
        startup.push(started);
        first.push(session.evaluate("40 + 2", "42")?);
        session.stop()?;
    }

    let mut changed = Vec::with_capacity(options.sample_count);
    let mut unchanged = Vec::with_capacity(options.sample_count);
    for index in 0..options.sample_count {
        let root = fresh_sample_directory(workspace, "warm_repl", index)?;
        let (mut session, _) = ReplSession::start(&options.terlc, &root)?;
        session.evaluate("40 + 2", "42")?;
        changed.push(session.evaluate("41 + 2", "43")?);
        unchanged.push(session.evaluate("41 + 2", "43")?);
        session.stop()?;
    }

    Ok(vec![
        terlan_only_measurement(
            "repl_startup",
            "fresh persistent compiler-service process start through initial ready prompt",
            startup,
        )?,
        terlan_only_measurement(
            "first_repl",
            "first expression declaration through registered native generation and rendered result",
            first,
        )?,
        terlan_only_measurement(
            "changed_repl",
            "changed expression through replacement native generation and rendered result in one persistent service",
            changed,
        )?,
        terlan_only_measurement(
            "unchanged_repl",
            "unchanged expression through active native generation reuse and rendered result in one persistent service",
            unchanged,
        )?,
    ])
}

/// Builds one comparable measurement and computes both Terlan-to-Go ratios.
fn comparable_measurement(
    name: &str,
    scope: &str,
    terlan: Vec<Duration>,
    go: Vec<Duration>,
) -> Result<CompilationMeasurement, String> {
    let terlan = CompilationTiming::from_durations(terlan)?;
    let go = CompilationTiming::from_durations(go)?;
    Ok(CompilationMeasurement {
        name: name.to_string(),
        scope: scope.to_string(),
        median_ratio: Some(ratio(terlan.median_ns, go.median_ns)),
        p95_ratio: Some(ratio(terlan.p95_ns, go.p95_ns)),
        terlan,
        go: Some(go),
        reference_note: None,
    })
}

/// Builds one Terlan-only measurement for operations absent from Go tooling.
fn terlan_only_measurement(
    name: &str,
    scope: &str,
    samples: Vec<Duration>,
) -> Result<CompilationMeasurement, String> {
    Ok(CompilationMeasurement {
        name: name.to_string(),
        scope: scope.to_string(),
        terlan: CompilationTiming::from_durations(samples)?,
        go: None,
        median_ratio: None,
        p95_ratio: None,
        reference_note: Some(
            "Go build has no persistent language REPL generation operation; no synthetic reference is reported"
                .to_string(),
        ),
    })
}

/// Copies one equivalent fixture pair into an isolated sample directory.
fn prepare_sample(
    options: &CompilationBenchmarkOptions,
    root: &Path,
    kind: FixtureKind,
) -> Result<SampleWorkspace, String> {
    let terlan_root = root.join("terlan");
    let go_root = root.join("go");
    let terlan_fixture = match kind {
        FixtureKind::Small => options.fixtures.join("terlan/small"),
        FixtureKind::Multi => options.fixtures.join("terlan/multi"),
    };
    copy_tree(&terlan_fixture, &terlan_root)?;
    copy_tree(&options.fixtures.join("go"), &go_root)?;
    let output = root.join("output");
    let (terlan_input, image_name, go_package) = match kind {
        FixtureKind::Small => (
            terlan_root.join("Small.terl"),
            "aotbench_Small.tvm",
            "./cmd/small",
        ),
        FixtureKind::Multi => (terlan_root.clone(), "aotbench_Main.tvm", "./cmd/app"),
    };
    Ok(SampleWorkspace {
        terlan_input,
        terlan_image: output.join("terlan/vm").join(image_name),
        go_root,
        go_package,
        go_output: output.join(executable_name("go-command")),
    })
}

/// Runs one measured Terlan build after all setup has completed.
fn run_terlan_build(
    options: &CompilationBenchmarkOptions,
    sample: &SampleWorkspace,
    release: bool,
) -> Result<Duration, String> {
    let out_dir = sample
        .terlan_image
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "Terlan image has no output directory".to_string())?;
    let mut command = Command::new(&options.terlc);
    command
        .arg("--incremental")
        .arg("build")
        .arg(&sample.terlan_input)
        .arg("--target")
        .arg("terlan-vm");
    if release {
        command.arg("--release");
    }
    command.arg("--out-dir").arg(out_dir);
    run_timed_command(&mut command, "Terlan build")
}

/// Runs one measured Go build against the equivalent command package.
fn run_go_build(
    options: &CompilationBenchmarkOptions,
    sample: &SampleWorkspace,
    release: bool,
) -> Result<Duration, String> {
    if let Some(parent) = sample.go_output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create Go output directory: {error}"))?;
    }
    let mut command = Command::new(&options.go);
    command.current_dir(&sample.go_root).env("GOWORK", "off");
    command.arg("build").arg("-buildvcs=false");
    if release {
        command.arg("-trimpath").arg("-ldflags=-s -w");
    }
    command
        .arg("-o")
        .arg(&sample.go_output)
        .arg(sample.go_package);
    run_timed_command(&mut command, "Go build")
}

/// Executes one command and returns wall latency only after successful exit.
fn run_timed_command(command: &mut Command, label: &str) -> Result<Duration, String> {
    let started = Instant::now();
    let output = command
        .output()
        .map_err(|error| format!("failed to start {label}: {error}"))?;
    let elapsed = started.elapsed();
    if !output.status.success() {
        return Err(format!(
            "{label} failed with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(elapsed)
}

/// Executes a produced Terlan image outside the timed compilation region.
fn verify_terlan_image(
    options: &CompilationBenchmarkOptions,
    sample: &SampleWorkspace,
    kind: FixtureKind,
) -> Result<(), String> {
    let entry = match kind {
        FixtureKind::Small => "aotbench.Small.main",
        FixtureKind::Multi => "aotbench.Main.main",
    };
    let output = Command::new(&options.terlan_vm)
        .arg("run")
        .arg(&sample.terlan_image)
        .arg("--entry")
        .arg(entry)
        .arg("--test-eval")
        .output()
        .map_err(|error| format!("failed to execute Terlan benchmark image: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Terlan benchmark image failed correctness execution:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Executes a produced Go command outside the timed compilation region.
fn verify_go_image(path: &Path) -> Result<(), String> {
    let status = Command::new(path)
        .status()
        .map_err(|error| format!("failed to execute Go benchmark command: {error}"))?;
    if !status.success() {
        return Err(format!("Go benchmark command exited with {status}"));
    }
    Ok(())
}

/// Applies one source mutation after cache population and before measurement.
fn apply_warm_mutation(sample: &SampleWorkspace, mutation: WarmMutation) -> Result<(), String> {
    match mutation {
        WarmMutation::None => Ok(()),
        WarmMutation::Dependency => {
            replace_text(
                &sample.terlan_input.join("src/aotbench/Math.terl"),
                "-> 7.",
                "-> 8.",
            )?;
            replace_text(
                &sample.go_root.join("internal/mathvalue/value.go"),
                "\treturn 7",
                "\treturn 8",
            )
        }
        WarmMutation::Root => {
            replace_text(
                &sample.terlan_input.join("src/aotbench/Main.terl"),
                "Math.value() + 34",
                "Math.value() + 35",
            )?;
            replace_text(
                &sample.go_root.join("cmd/app/main.go"),
                "mathvalue.Value()+34",
                "mathvalue.Value()+35",
            )
        }
    }
}

/// Adds harmless run-unique comments so Go recompiles all fixture packages.
fn add_sample_nonce(
    sample: &SampleWorkspace,
    kind: FixtureKind,
    index: usize,
) -> Result<(), String> {
    let mut go_sources = match kind {
        FixtureKind::Small => vec![sample.go_root.join("cmd/small/main.go")],
        FixtureKind::Multi => vec![
            sample.go_root.join("cmd/app/main.go"),
            sample.go_root.join("internal/mathvalue/value.go"),
        ],
    };
    go_sources.sort();
    let workspace_identity = sha256(sample.go_root.to_string_lossy().as_bytes());
    for go_source in go_sources {
        let mut source = fs::read_to_string(&go_source)
            .map_err(|error| format!("failed to read Go nonce source: {error}"))?;
        source.push_str(&format!(
            "\n// benchmark sample {index} {}\n",
            &workspace_identity[..16]
        ));
        fs::write(&go_source, source)
            .map_err(|error| format!("failed to write Go nonce source: {error}"))?;
    }
    Ok(())
}

/// Replaces exactly one expected fixture fragment.
fn replace_text(path: &Path, before: &str, after: &str) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
    if source.matches(before).count() != 1 {
        return Err(format!(
            "fixture `{}` does not contain exactly one `{before}` mutation target",
            path.display()
        ));
    }
    fs::write(path, source.replacen(before, after, 1))
        .map_err(|error| format!("failed to write `{}`: {error}", path.display()))
}

/// Creates a clean sample directory with a stable scenario/index name.
fn fresh_sample_directory(
    workspace: &Path,
    scenario: &str,
    index: usize,
) -> Result<PathBuf, String> {
    let path = workspace.join(format!("{scenario}-{index}"));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create sample directory: {error}"))?;
    Ok(path)
}

/// Recursively copies a committed fixture tree without shelling out.
fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("failed to create `{}`: {error}", destination.display()))?;
    let mut entries = fs::read_dir(source)
        .map_err(|error| format!("failed to read `{}`: {error}", source.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read fixture entry: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|error| {
                format!(
                    "failed to copy `{}` to `{}`: {error}",
                    from.display(),
                    to.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Computes one deterministic digest over sorted fixture paths and contents.
fn fixture_sha256(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut canonical = Vec::new();
    for (path, bytes) in files {
        canonical.extend_from_slice(path.as_bytes());
        canonical.push(0);
        canonical.extend_from_slice(&bytes);
        canonical.push(0);
    }
    Ok(sha256(&canonical))
}

/// Collects relative file names and contents for fixture hashing.
fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("failed to read `{}`: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read fixture entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("failed to relativize fixture path: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(&path)
                .map_err(|error| format!("failed to read `{}`: {error}", path.display()))?;
            files.push((relative, bytes));
        }
    }
    Ok(())
}

/// Validates report completeness without imposing machine-specific ratios yet.
pub(crate) fn validate_report(report: &CompilationBenchmarkReport) -> Result<(), String> {
    if report.schema != REPORT_SCHEMA || report.status != "completed" {
        return Err("AOT compilation report schema or status is invalid".to_string());
    }
    if report.sample_count == 0
        || report.toolchains.go == "unknown"
        || report.toolchains.terlc_sha256.len() != 64
        || report.hardware.sha256.len() != 64
        || report.fixtures.sha256.len() != 64
        || report.fixtures.workloads != ["small-command", "multi-package"]
        || report.cache_state.dependency_downloads_timed
    {
        return Err("AOT compilation report identity evidence is incomplete".to_string());
    }
    let expected = [
        "small_cold_development",
        "multi_cold_development",
        "one_package_edit",
        "no_op_development",
        "cold_release",
        "package_relink",
        "repl_startup",
        "first_repl",
        "changed_repl",
        "unchanged_repl",
    ];
    if report
        .measurements
        .iter()
        .map(|measurement| measurement.name.as_str())
        .collect::<Vec<_>>()
        != expected
    {
        return Err("AOT compilation report measurement order is incomplete".to_string());
    }
    for measurement in &report.measurements {
        validate_timing(&measurement.terlan, report.sample_count)?;
        match &measurement.go {
            Some(go) => {
                validate_timing(go, report.sample_count)?;
                let median_ratio = measurement.median_ratio.ok_or_else(|| {
                    format!(
                        "comparable measurement `{}` has no median ratio",
                        measurement.name
                    )
                })?;
                let p95_ratio = measurement.p95_ratio.ok_or_else(|| {
                    format!(
                        "comparable measurement `{}` has no p95 ratio",
                        measurement.name
                    )
                })?;
                if !ratio_matches(
                    median_ratio,
                    ratio(measurement.terlan.median_ns, go.median_ns),
                ) || !ratio_matches(p95_ratio, ratio(measurement.terlan.p95_ns, go.p95_ns))
                    || measurement.reference_note.is_some()
                {
                    return Err(format!(
                        "comparable measurement `{}` has invalid ratios",
                        measurement.name
                    ));
                }
            }
            None => {
                if measurement.median_ratio.is_some()
                    || measurement.p95_ratio.is_some()
                    || measurement.reference_note.is_none()
                {
                    return Err(format!(
                        "Terlan-only measurement `{}` has synthetic reference data",
                        measurement.name
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Compares one serialized ratio with its timing-derived value.
fn ratio_matches(actual: f64, expected: f64) -> bool {
    actual.is_finite()
        && actual > 0.0
        && (actual - expected).abs() <= f64::EPSILON * expected.abs().max(1.0) * 4.0
}

/// Validates one sorted non-zero timing summary and its percentile fields.
fn validate_timing(timing: &CompilationTiming, sample_count: usize) -> Result<(), String> {
    if timing.samples_ns.len() != sample_count
        || timing.samples_ns.contains(&0)
        || !timing.samples_ns.windows(2).all(|pair| pair[0] <= pair[1])
        || timing.min_ns != timing.samples_ns[0]
        || timing.max_ns != timing.samples_ns[timing.samples_ns.len() - 1]
        || timing.median_ns != percentile(&timing.samples_ns, 50)
        || timing.p95_ns != percentile(&timing.samples_ns, 95)
    {
        return Err("AOT compilation timing summary is invalid".to_string());
    }
    Ok(())
}

/// Writes a pretty JSON report after creating its parent directory.
fn write_report(path: &Path, report: &CompilationBenchmarkReport) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create `{}`: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to serialize compilation report: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("failed to write `{}`: {error}", path.display()))
}

/// Returns a nearest-rank percentile from a sorted non-empty sample set.
fn percentile(sorted: &[u128], requested: usize) -> u128 {
    let rank = sorted.len() * requested;
    let index = rank.div_ceil(100).saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

/// Computes a finite Terlan-to-Go duration ratio.
fn ratio(terlan: u128, go: u128) -> f64 {
    terlan as f64 / go.max(1) as f64
}

/// Computes the SHA-256 of one required benchmark input file.
fn sha256_file(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| sha256(&bytes))
        .map_err(|error| format!("failed to read `{}`: {error}", path.display()))
}

/// Creates one unique temporary benchmark workspace.
fn create_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-aot-compilation-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create benchmark workspace: {error}"))?;
    Ok(path)
}

/// Returns the current Unix timestamp in seconds.
fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Returns a platform executable file name.
fn executable_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}
mod self_test;
