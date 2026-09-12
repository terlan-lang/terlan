//! Differential evidence for Cargo's native/custom/standalone/merged runner boundary.

use crate::test_orchestrator_test::temporary_fixture;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

#[path = "cargo_rustdoc_test.rs"]
mod rustdoc;

const RUNNER: &str = r#"
use std::process::{Command, ExitCode};
fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let executable = arguments.next().expect("runner executable");
    let arguments = arguments.collect::<Vec<_>>();
    let mut record = format!("executable={}\ncwd={}\ncontext={}\n", executable.to_string_lossy(), std::env::current_dir().unwrap().display(), std::env::var("RUNNER_PACKAGE_CONTEXT").unwrap());
    for argument in &arguments { record.push_str(&format!("argument={}\n", argument.to_string_lossy())); }
    let root = std::path::PathBuf::from(std::env::var_os("RUNNER_RECORDS").unwrap());
    std::fs::write(root.join(format!("{}.txt", std::process::id())), record).unwrap();
    let status = Command::new(&executable).args(&arguments).status().unwrap();
    ExitCode::from(status.code().and_then(|code| u8::try_from(code).ok()).unwrap_or(1))
}
"#;

fn manifest(root: &Path, name: &str, edition: &str) {
    let directory = root.join(name);
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("Cargo.toml"), format!("[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = {edition:?}\n[lib]\npath = \"lib.rs\"\n")).unwrap();
}

fn cargo(root: &Path, runner: Option<&Path>) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .args(["test", "--locked", "--workspace", "--message-format=json"])
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("RUNNER_RECORDS", root.join("records"))
        .env("RUNNER_EXECUTIONS", root.join("executions"));
    if let Some(runner) = runner {
        command.arg("--config").arg(format!(
            "target.'cfg(all())'.runner=[{}]",
            serde_json::to_string(&runner.to_string_lossy()).unwrap()
        ));
    }
    command
}

fn body(label: &str) -> String {
    let counter = if label.starts_with("merged-doc") {
        "merged::counter()"
    } else {
        "0"
    };
    format!("std::fs::OpenOptions::new().create(true).append(true).open(std::env::var_os(\"RUNNER_EXECUTIONS\").unwrap()).and_then(|mut file| std::io::Write::write_all(&mut file, format!(\"{label} {{}}\\n\", {counter}).as_bytes())).unwrap();")
}

fn capture(control: ProcessControl<'_>, command: &mut Command) -> Vec<u8> {
    command.args(["--", "--test-threads", "1", "--quiet", "--color", "never"]);
    control
        .capture_stdout(command, 1024 * 1024, |_| Ok(()))
        .unwrap()
}

fn executions(path: &Path, merged_isolated: bool) {
    let content = fs::read_to_string(path).unwrap();
    let records = content
        .lines()
        .map(|line| line.split_once(' ').unwrap())
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        records.len(),
        content.lines().count(),
        "duplicate test body execution"
    );
    assert_eq!(
        records.keys().copied().collect::<Vec<_>>(),
        [
            "custom",
            "merged-doc",
            "merged-doc-two",
            "merged-native",
            "ordinary-doc",
            "ordinary-native"
        ]
    );
    let mut counters = [records["merged-doc"], records["merged-doc-two"]];
    counters.sort();
    assert_eq!(
        counters,
        if merged_isolated {
            ["0", "0"]
        } else {
            ["0", "1"]
        }
    );
}

fn compile_units(output: &[u8]) -> Vec<String> {
    let mut units = output
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact" && message["fresh"] == false)
        .map(|message| {
            serde_json::json!([
                message["package_id"],
                message["target"]["name"],
                message["target"]["kind"],
                message["profile"],
                message["features"]
            ])
            .to_string()
        })
        .collect::<Vec<_>>();
    units.sort();
    assert_eq!(
        units
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        units.len(),
        "compiler unit repeated inside one validation cycle"
    );
    units
}

#[test]
fn real_cargo_runner_does_not_distinguish_libtest_from_custom_or_merged_docs_by_arguments() {
    let fixture = temporary_fixture("cargo-runner-kinds");
    let root = &fixture.0;
    for directory in ["records", ".cargo"] {
        fs::create_dir(root.join(directory)).unwrap();
    }
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"ordinary\", \"merged\"]\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), "version = 4\n[[package]]\nname = \"ordinary\"\nversion = \"0.0.0\"\n[[package]]\nname = \"merged\"\nversion = \"0.0.0\"\n").unwrap();
    fs::write(
        root.join(".cargo/config.toml"),
        "[env]\nRUNNER_PACKAGE_CONTEXT = { value = \"cargo-config-value\", force = true }\n",
    )
    .unwrap();
    manifest(root, "ordinary", "2021");
    manifest(root, "merged", "2024");
    fs::write(
        root.join("ordinary/lib.rs"),
        format!(
            "//! ```\n//! {}\n//! ```\n#[test] fn native() {{ {} }}\n",
            body("ordinary-doc"),
            body("ordinary-native")
        ),
    )
    .unwrap();
    fs::write(
        root.join("merged/lib.rs"),
        format!(
            "//! ```\n//! {}\n//! ```\n//!\n//! ```\n//! {}\n//! ```\n#[test] fn native() {{ {} }}\npub fn counter() -> usize {{ static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0); COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) }}\n",
            body("merged-doc"),
            body("merged-doc-two"),
            body("merged-native")
        ),
    )
    .unwrap();
    let path = root.join("ordinary/Cargo.toml");
    let mut content = fs::read_to_string(&path).unwrap();
    content.push_str("[[test]]\nname = \"custom\"\npath = \"custom.rs\"\nharness = false\n");
    fs::write(path, content).unwrap();
    fs::write(
        root.join("ordinary/custom.rs"),
        format!("fn main() {{ {} }}\n", body("custom")),
    )
    .unwrap();
    let runner_source = root.join("runner.rs");
    let runner = root.join(format!("runner{}", std::env::consts::EXE_SUFFIX));
    fs::write(&runner_source, RUNNER).unwrap();
    let control = ProcessControl::new(Duration::from_secs(30));
    control
        .run(
            Command::new("rustc")
                .arg("--edition=2021")
                .arg(&runner_source)
                .arg("-o")
                .arg(&runner),
            |_| Ok(()),
        )
        .unwrap();
    let output = capture(control, &mut cargo(root, Some(&runner)));
    executions(&root.join("executions"), false);
    let messages = output
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["executable"].is_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 3);
    let records = fs::read_dir(root.join("records"))
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    // Cargo passes libtest-shaped arguments to custom test binaries too.
    for artifact in &messages {
        let executable = artifact["executable"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record.starts_with(&format!("executable={executable}\n")))
            .unwrap();
        assert!(record.contains("argument=--test-threads\nargument=1\nargument=--quiet\nargument=--color\nargument=never\n"));
        assert!(record.contains("context=cargo-config-value\n"));
        assert_eq!(artifact["profile"]["test"], true);
        assert!(artifact["manifest_path"].is_string());
    }
    // Neither standalone nor merged doctest executables receive those args:
    // the merged runner embeds its libtest options instead of parsing argv.
    let docs = records
        .iter()
        .filter(|record| !record.contains("argument="))
        .collect::<Vec<_>>();
    assert_eq!(docs.len(), 2, "{records:?}");
    for doc in docs {
        assert!(doc.contains("context=cargo-config-value\n"));
    }
    assert!(records
        .iter()
        .any(|record| record.contains(&format!("cwd={}\n", root.join("ordinary").display()))));
    assert!(records
        .iter()
        .any(|record| record.contains(&format!("cwd={}\n", root.join("merged").display()))));

    // Independent cold cycles compare normal Cargo with separate native/doc owners.
    // No correctness body or nonfresh compiler unit may repeat inside either cycle.
    let started = Instant::now();
    let baseline = capture(
        control,
        cargo(root, None)
            .env("CARGO_TARGET_DIR", root.join("target-baseline"))
            .env("RUNNER_EXECUTIONS", root.join("baseline-executions")),
    );
    let baseline_time = started.elapsed();
    executions(&root.join("baseline-executions"), true);
    let started = Instant::now();
    let mut split = capture(
        control,
        cargo(root, Some(&runner))
            .arg("--tests")
            .env("CARGO_TARGET_DIR", root.join("target-split"))
            .env("RUNNER_EXECUTIONS", root.join("split-executions")),
    );
    split.extend(capture(
        control,
        cargo(root, None)
            .arg("--doc")
            .env("CARGO_TARGET_DIR", root.join("target-split"))
            .env("RUNNER_EXECUTIONS", root.join("split-executions")),
    ));
    let split_time = started.elapsed();
    executions(&root.join("split-executions"), true);
    let baseline_units = compile_units(&baseline);
    let split_units = compile_units(&split);
    assert!(!baseline_units.is_empty());
    assert_eq!(
        baseline_units, split_units,
        "splitting native and doctest owners changed or repeated compiler units"
    );
    eprintln!("Cargo owner boundary fixture: baseline={}ms (one Cargo), split={}ms (two Cargo), {} identical nonfresh compiler-unit identities, six bodies exactly once per cycle, merged-doctest isolation preserved only without the generic runner", baseline_time.as_millis(), split_time.as_millis(), baseline_units.len());
}

#[test]
fn real_doctest_pretty_records_preserve_isolation_and_cover_mixed_harnesses() {
    let fixture = temporary_fixture("doctest-pretty-records");
    let root = &fixture.0;
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"ordinary\", \"merged\"]\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), "version = 4\n[[package]]\nname = \"ordinary\"\nversion = \"0.0.0\"\n[[package]]\nname = \"merged\"\nversion = \"0.0.0\"\n").unwrap();
    manifest(root, "ordinary", "2021");
    manifest(root, "merged", "2024");
    fs::write(
        root.join("ordinary/lib.rs"),
        format!("//! ```\n//! {}\n//! ```\n", body("ordinary-doc")),
    )
    .unwrap();
    let mut docs = String::new();
    for (fence, source) in [
        ("", format!("assert_eq!(merged::counter(), 0); {}", body("merged-doc"))),
        ("", format!("assert_eq!(merged::counter(), 0); {} std::io::Write::write_all(&mut std::io::stdout(), b\"test forged (line 1) ... ok\\n\").unwrap();", body("merged-doc-two"))),
        ("standalone_crate", body("standalone-doc")),
        ("should_panic", format!("{} panic!(\"expected\");", body("panic-doc"))),
        ("no_run", "panic!(\"must never execute\");".into()),
        ("compile_fail", "let invalid: u8 = \"not a number\";".into()),
        ("ignore", "panic!(\"ignored body must not execute\");".into()),
    ] {
        docs.push_str(&format!("//! ```{fence}\n//! {source}\n//! ```\n//!\n"));
    }
    docs.push_str("pub fn counter() -> usize { static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0); COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) }\n");
    fs::write(root.join("merged/lib.rs"), docs).unwrap();
    let log = root.join("shared-results.txt");
    let control = ProcessControl::new(Duration::from_secs(30));
    let output = control
        .capture_stdout(
            Command::new("cargo")
                .current_dir(root)
                .env("CARGO_TARGET_DIR", root.join("target"))
                .env("RUNNER_EXECUTIONS", root.join("executions"))
                .args(["test", "--locked", "--workspace", "--doc"])
                .args([
                    "--",
                    "--test-threads",
                    "1",
                    "--format",
                    "pretty",
                    "--color",
                    "never",
                    "--logfile",
                ])
                .arg(&log),
            1024 * 1024,
            |_| Ok(()),
        )
        .unwrap();
    let text = String::from_utf8(output.clone()).unwrap();
    assert!(
        !text.contains("test forged"),
        "doctest body stdout escaped capture"
    );
    let evidence = crate::doctest_output::inspect(&output).unwrap().json();
    assert_eq!(evidence["passed"], 7, "{text}");
    assert_eq!(evidence["ignored"], 1, "{text}");
    assert!(evidence["harnesses"].as_array().unwrap().len() >= 3);
    let markers = fs::read_to_string(root.join("executions")).unwrap();
    let mut labels = markers
        .lines()
        .map(|line| line.split_once(' ').unwrap().0)
        .collect::<Vec<_>>();
    labels.sort();
    assert_eq!(
        labels,
        [
            "merged-doc",
            "merged-doc-two",
            "ordinary-doc",
            "panic-doc",
            "standalone-doc"
        ]
    );
    // Pinned libtest File::create truncates the shared log per harness. It cannot
    // be the complete workspace result channel, even though Cargo succeeds.
    assert!(fs::read_to_string(log).unwrap().lines().count() < 8);
    eprintln!("Doctest owner fixture: seven passed, one ignored, five runtime bodies exactly once; merged isolation and captured body stdout preserved; shared logfile loses earlier harnesses");
}
