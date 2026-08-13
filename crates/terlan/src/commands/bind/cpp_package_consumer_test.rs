//! Full-cycle execution of a generated C++ binding as an external Git package.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

const GENERATED_PACKAGE_REPORT_SCHEMA: &str = "terlan.cpp.generated-package-report.v1";

#[derive(Serialize)]
struct GeneratedPackageReport {
    schema: &'static str,
    fixture: &'static str,
    fixture_classification: &'static str,
    package_tests: &'static str,
    adapter_rust_tests: &'static str,
    package_consumer: &'static str,
    stale_handle: &'static str,
    torch_like_package_tests: &'static str,
    torch_fixture_classification: &'static str,
    torch_namespace_import: &'static str,
    skipped_symbols_sha256: String,
    torch_skipped_symbols_sha256: String,
    skipped_symbol_count: usize,
    policy_reasons: Vec<String>,
    policy_messages: Vec<String>,
    artifact_hashes: BTreeMap<String, String>,
    torch_artifact_hashes: BTreeMap<String, String>,
}

/// Proves generated C++ packages build and execute through public package commands.
#[test]
#[ignore = "requires built terlc and terlan-vm binaries; run make cpp-package-consumer-check"]
fn generated_cpp_git_package_executes_and_rejects_stale_handles() {
    let root = temporary_root();
    let package = root.join("native_boundary_package");
    let generated_adapter_target = root.join("generated-native-target");
    let torch_package = root.join("torch_namespace_package");
    let torch_adapter_target = root.join("torch-native-target");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cpp_native_boundary");
    let terlc = built_binary("terlc");
    let vm = built_binary("terlan-vm");
    assert!(terlc.is_file(), "missing terlc test binary");
    assert!(vm.is_file(), "missing terlan-vm test binary");

    assert_success(
        command(&terlc)
            .args(["bind", "cpp", "--manifest"])
            .arg(fixture.join("native-binding.json"))
            .args(["--out"])
            .arg(&package)
            .output()
            .expect("run terlc bind cpp"),
        "generate external C++ package",
    );
    assert_package_is_location_independent(&package, &fixture);
    let torch_manifest = write_torch_style_fixture(&fixture, &root);
    assert_success(
        command(&terlc)
            .args(["bind", "cpp", "--manifest"])
            .arg(&torch_manifest)
            .args(["--out"])
            .arg(&torch_package)
            .output()
            .expect("generate torch-style external namespace package"),
        "generate torch-style external namespace package",
    );
    assert_package_is_location_independent(
        &torch_package,
        torch_manifest.parent().expect("torch fixture directory"),
    );
    assert_torch_namespace_is_external(&torch_package);
    let report = generated_package_report(&package, &torch_package);
    execute_generated_package_tests(
        &terlc,
        &package,
        &generated_adapter_target,
        "TERLAN_CPP_FIXTURE_NATIVE_BOUNDARY_HELPER_PATH",
    );
    execute_generated_package_tests(
        &terlc,
        &torch_package,
        &torch_adapter_target,
        "TERLAN_TORCH_NATIVE_BOUNDARY_HELPER_PATH",
    );
    let revision = commit_package(&package);

    let consumer = root.join("consumer");
    write_consumer(&consumer, &package, &revision, valid_consumer_source());
    assert_success(
        command(&terlc)
            .args(["package", "fetch"])
            .arg(&consumer)
            .output()
            .expect("fetch generated package"),
        "fetch generated package",
    );
    assert!(consumer.join("terlan.lock").is_file());
    fs::remove_dir_all(&package).expect("remove original package repository");

    let valid_build = root.join("valid-build");
    let valid = run_consumer(&terlc, &consumer, &valid_build);
    assert_success(valid.clone(), "run external C++ package consumer");
    let valid_stdout = String::from_utf8_lossy(&valid.stdout);
    assert!(
        valid_stdout.contains("CPP_PACKAGE_CONSUMER_OK"),
        "consumer did not execute its native lifecycle:\n{valid_stdout}"
    );
    assert!(!valid_stdout.contains("CPP_PACKAGE_CONSUMER_FAILED"));
    let native_target = assert_native_dependency_metadata(&valid_build, &consumer, &revision);

    fs::write(
        consumer.join("src/cpp_consumer/Main.terl"),
        stale_consumer_source(),
    )
    .expect("write stale-handle consumer");
    let stale = run_consumer(&terlc, &consumer, &root.join("stale-build"));
    assert!(
        !stale.status.success(),
        "stale native handle unexpectedly succeeded:\n{}",
        render_output(&stale)
    );
    let stale_output = render_output(&stale);
    assert!(
        stale_output.contains("stale_handle"),
        "missing stable stale-handle diagnostic:\n{stale_output}"
    );
    assert!(!stale_output.contains("STALE_HANDLE_NOT_REJECTED"));

    fs::remove_dir_all(&native_target).expect("remove first native build target");
    fs::remove_dir_all(&valid_build).expect("remove first consumer build");
    fs::write(
        consumer.join("src/cpp_consumer/Main.terl"),
        valid_consumer_source(),
    )
    .expect("restore valid consumer");
    let rebuilt = run_consumer(&terlc, &consumer, &valid_build);
    assert_success(
        rebuilt.clone(),
        "rebuild external package from verified cache",
    );
    assert!(String::from_utf8_lossy(&rebuilt.stdout).contains("CPP_PACKAGE_CONSUMER_OK"));
    assert!(native_target.join("debug/native-boundary-helper").is_file());

    write_generated_package_report(&report);
    fs::remove_dir_all(root).expect("remove external package test workspace");
}

fn generated_package_report(package: &Path, torch_package: &Path) -> GeneratedPackageReport {
    let skipped_path = package.join("bindings/skipped-symbols.json");
    let skipped_bytes = fs::read(&skipped_path).expect("read skipped-symbol snapshot");
    let skipped: serde_json::Value =
        serde_json::from_slice(&skipped_bytes).expect("parse skipped-symbol snapshot");
    let entries = skipped["skipped"].as_array().expect("skipped symbol array");
    let policy_reasons = entries
        .iter()
        .map(|entry| {
            entry["reason"]
                .as_str()
                .expect("machine-readable skip reason")
                .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let policy_messages = entries
        .iter()
        .map(|entry| {
            entry["message"]
                .as_str()
                .expect("generator-owned skip message")
                .to_string()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut artifact_hashes = BTreeMap::new();
    for relative in [
        "terlan.toml",
        "bindings/native-binding-manifest.json",
        "bindings/skipped-symbols.json",
        "docs/cpp_fixture.NativeBoundary.md",
        "docs/cpp_fixture.NativeGauge.md",
        "src/cpp_fixture/NativeBoundary.terl",
        "src/cpp_fixture/NativeGauge.terl",
        "tests/cpp_fixture/NativeBoundaryTest.terl",
        "tests/cpp_fixture/NativeGaugeTest.terl",
    ] {
        artifact_hashes.insert(
            relative.to_string(),
            sha256(&fs::read(package.join(relative)).expect("read generated artifact")),
        );
    }
    let torch_skipped_bytes = fs::read(torch_package.join("bindings/skipped-symbols.json"))
        .expect("read torch-style skipped-symbol snapshot");
    let mut torch_artifact_hashes = BTreeMap::new();
    for relative in [
        "terlan.toml",
        "bindings/native-binding-manifest.json",
        "bindings/skipped-symbols.json",
        "docs/torch.NativeBoundary.md",
        "docs/torch.NativeGauge.md",
        "src/torch/NativeBoundary.terl",
        "src/torch/NativeGauge.terl",
        "tests/torch/NativeBoundaryTest.terl",
        "tests/torch/NativeGaugeTest.terl",
    ] {
        torch_artifact_hashes.insert(
            relative.to_string(),
            sha256(&fs::read(torch_package.join(relative)).expect("read torch-style artifact")),
        );
    }
    GeneratedPackageReport {
        schema: GENERATED_PACKAGE_REPORT_SCHEMA,
        fixture: "cpp_native_boundary",
        fixture_classification: "pass_with_reviewed_skips",
        package_tests: "passed",
        adapter_rust_tests: "passed",
        package_consumer: "passed",
        stale_handle: "expected_failure",
        torch_like_package_tests: "passed",
        torch_fixture_classification: "pass_with_reviewed_skips",
        torch_namespace_import: "torch.NativeBoundary",
        skipped_symbols_sha256: sha256(&skipped_bytes),
        torch_skipped_symbols_sha256: sha256(&torch_skipped_bytes),
        skipped_symbol_count: entries.len(),
        policy_reasons,
        policy_messages,
        artifact_hashes,
        torch_artifact_hashes,
    }
}

fn write_generated_package_report(report: &GeneratedPackageReport) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let quality = workspace.join("target/quality");
    fs::create_dir_all(&quality).expect("create quality report directory");
    let value = serde_json::to_value(report).expect("serialize generated-package report");
    let mut encoded =
        serde_json::to_string_pretty(&value).expect("render generated-package report");
    encoded.push('\n');
    fs::write(
        quality.join("cpp-binding-generator.gen_report.json"),
        encoded,
    )
    .expect("write generated-package report");
}

fn sha256(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut encoded, "{byte:02x}").expect("write SHA-256");
    }
    encoded
}

/// Resolves one binary built beside the current hashed `terlc` test executable.
fn built_binary(name: &str) -> PathBuf {
    let test_binary = std::env::current_exe().expect("resolve current test executable");
    let debug_dir = test_binary
        .parent()
        .and_then(Path::parent)
        .expect("resolve Cargo debug directory");
    debug_dir.join(name)
}

/// Returns a command with environment isolation required by this package test.
fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_remove("TERLAN_CPP_FIXTURE_NATIVE_BOUNDARY_HELPER_PATH")
        .env("CARGO_NET_OFFLINE", "true");
    command
}

/// Creates a collision-resistant temporary workspace for the external package.
fn temporary_root() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_cpp_package_consumer_{}_{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&root).expect("create external package workspace");
    root
}

/// Generates the adapter's Cargo lockfile before publishing its immutable tree.
fn generate_native_lockfile(package: &Path) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    assert_success(
        Command::new(cargo)
            .args(["generate-lockfile", "--manifest-path"])
            .arg(package.join("native/rust/Cargo.toml"))
            .arg("--offline")
            .output()
            .expect("generate native adapter lockfile"),
        "generate native adapter lockfile",
    );
}

/// Runs the adapter and Terlan-facing tests for one freshly generated package.
fn execute_generated_package_tests(
    terlc: &Path,
    package: &Path,
    target: &Path,
    helper_environment: &str,
) {
    generate_native_lockfile(package);
    assert_success(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .env("CARGO_TARGET_DIR", target)
            .args(["test", "--manifest-path"])
            .arg(package.join("native/rust/Cargo.toml"))
            .arg("--offline")
            .output()
            .expect("execute generated adapter Rust tests"),
        "execute generated adapter Rust tests",
    );
    build_native_helper(package, target);
    let helper = target.join("debug/native-boundary-helper");
    assert!(helper.is_file(), "generated native helper was not built");
    assert_success(
        command(terlc)
            .env(helper_environment, &helper)
            .args(["test"])
            .arg(package)
            .args(["--target", "terlan-vm"])
            .output()
            .expect("execute generated package tests"),
        "execute generated Terlan package tests",
    );
}

/// Builds the package-owned helper used by package-mode Terlan tests.
fn build_native_helper(package: &Path, target: &Path) {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    assert_success(
        Command::new(cargo)
            .env("CARGO_TARGET_DIR", target)
            .args(["build", "--manifest-path"])
            .arg(package.join("native/rust/Cargo.toml"))
            .args(["--bin", "native-boundary-helper", "--offline"])
            .output()
            .expect("build native adapter helper"),
        "build native adapter helper",
    );
}

/// Creates a package-neutral variant whose public modules use a torch-like namespace.
fn write_torch_style_fixture(fixture: &Path, root: &Path) -> PathBuf {
    let target = root.join("torch-fixture");
    fs::create_dir_all(&target).expect("create torch-style fixture");
    for source in ["native_boundary.cc", "native_boundary.hpp"] {
        fs::copy(fixture.join(source), target.join(source)).expect("copy torch-style C++ fixture");
    }
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.join("native-binding.json")).expect("read C++ fixture manifest"),
    )
    .expect("parse C++ fixture manifest");
    manifest["package"]["namespace"] = "torch".into();
    manifest["package"]["crate_name"] = "terlan-torch-namespace-fixture".into();
    for module in manifest["modules"]
        .as_array_mut()
        .expect("fixture module array")
    {
        let name = module["module"].as_str().expect("fixture module name");
        module["module"] = name.replacen("cpp_fixture.", "torch.", 1).into();
        for function in module["functions"]
            .as_array_mut()
            .expect("fixture functions")
        {
            let operation = function["operation"]
                .as_str()
                .expect("fixture native operation");
            function["operation"] = operation.replacen("cpp_fixture.", "torch.", 1).into();
        }
    }
    let path = target.join("native-binding.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&manifest).expect("render torch-style fixture"),
    )
    .expect("write torch-style fixture");
    path
}

/// Proves generated imports remain in the package namespace.
fn assert_torch_namespace_is_external(package: &Path) {
    let source =
        fs::read_to_string(package.join("src/torch/NativeBoundary.terl")).expect("torch source");
    let test = fs::read_to_string(package.join("tests/torch/NativeBoundaryTest.terl"))
        .expect("torch package test");
    assert!(source.contains("module torch.NativeBoundary."));
    assert!(test.contains("import torch.NativeBoundary."));
    assert!(!source.contains("std.native"));
    assert!(!test.contains("std.native"));
}

/// Commits the generated package and returns its immutable Git revision.
fn commit_package(package: &Path) -> String {
    run_git(package, &["init", "--quiet"]);
    run_git(
        package,
        &["config", "user.email", "terlan-tests@example.invalid"],
    );
    run_git(package, &["config", "user.name", "Terlan Tests"]);
    run_git(package, &["add", "."]);
    run_git(package, &["commit", "--quiet", "-m", "generated package"]);
    git_output(package, &["rev-parse", "HEAD"])
}

/// Runs one Git command and requires successful completion.
fn run_git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .expect("run git command");
    assert_success(output, "prepare generated package repository");
}

/// Runs one Git query and returns trimmed UTF-8 output.
fn git_output(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .expect("run git query");
    assert_success(output.clone(), "query generated package repository");
    String::from_utf8(output.stdout)
        .expect("Git query output is UTF-8")
        .trim()
        .to_string()
}

/// Writes a manifest-backed executable that consumes the generated Git package.
fn write_consumer(consumer: &Path, package: &Path, revision: &str, source: &str) {
    fs::create_dir_all(consumer.join("src/cpp_consumer")).expect("create consumer source root");
    fs::write(
        consumer.join("terlan.toml"),
        format!(
            "[package]\nname = \"cpp-consumer\"\nversion = \"0.0.0\"\nnamespace = \"cpp_consumer\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n\n[dependencies]\ncpp_fixture = {{ git = {:?}, rev = {:?} }}\n",
            package.display().to_string(),
            revision
        ),
    )
    .expect("write consumer manifest");
    fs::write(consumer.join("src/cpp_consumer/Main.terl"), source).expect("write consumer source");
}

/// Executes the consumer through the public `terlc run` package path.
fn run_consumer(terlc: &Path, consumer: &Path, out_dir: &Path) -> Output {
    command(terlc)
        .arg("run")
        .arg(consumer)
        .args(["--target", "terlan-vm", "--out-dir"])
        .arg(out_dir)
        .output()
        .expect("run external package consumer")
}

/// Verifies emitted metadata points only into the fetched package cache.
fn assert_native_dependency_metadata(build: &Path, consumer: &Path, revision: &str) -> PathBuf {
    let text = fs::read_to_string(build.join("terlan-package-build.json"))
        .expect("read package build metadata");
    let metadata: serde_json::Value = serde_json::from_str(&text).expect("parse build metadata");
    let rust = &metadata["native"]["rust_dependencies"][0]["rust"];
    assert_eq!(rust["helper"], "native-boundary-helper");
    assert_eq!(
        rust["helper_env"],
        "TERLAN_CPP_FIXTURE_NATIVE_BOUNDARY_HELPER_PATH"
    );
    let package_dir = PathBuf::from(rust["package_dir"].as_str().expect("package directory"));
    assert!(package_dir.starts_with(consumer.join(".terlan/packages/git")));
    assert_eq!(
        package_dir.file_name().and_then(|name| name.to_str()),
        Some(revision)
    );
    let target = PathBuf::from(
        rust["target_dir"]
            .as_str()
            .expect("native target directory"),
    );
    assert!(target.starts_with(consumer.join(".terlan/packages/native-targets")));
    assert!(target.join("debug/native-boundary-helper").is_file());
    target
}

/// Rejects generated text that embeds the compiler checkout or fixture location.
fn assert_package_is_location_independent(package: &Path, fixture: &Path) {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("canonical compiler package");
    let fixture = fixture.canonicalize().expect("canonical fixture");
    for file in generated_files(package) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        assert!(
            !text.contains(&compiler.display().to_string()),
            "{} embeds compiler checkout",
            file.display()
        );
        assert!(
            !text.contains(&fixture.display().to_string()),
            "{} embeds fixture checkout",
            file.display()
        );
    }
}

/// Collects every generated file before the package becomes a Git repository.
fn generated_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("read generated package directory") {
            let entry = entry.expect("read generated package entry");
            if entry
                .file_type()
                .expect("read generated entry type")
                .is_dir()
            {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files
}

/// Returns a consumer that exercises conversions and the complete valid lifecycle.
fn valid_consumer_source() -> &'static str {
    r#"module cpp_consumer.Main.

import cpp_fixture.NativeBoundary.{Doubled, NativeSnapshot, Offset, Raw, add, bytes, dispose, label, live_count, mode, new, owned_snapshot, samples, snapshot, sum_floats, sum_integers, tripled_or_error, value}.
import std.io.Console.{println}.

valid_lifecycle(): Bool ->
    let boundary = new(40);
    add(boundary, 2);
    let observed = value(boundary);
    let copied = snapshot(boundary);
    let owned = owned_snapshot(7);
    let copied_label = label(boundary);
    let copied_bytes = bytes(boundary);
    let copied_samples = samples(boundary);
    let copied_mode = mode(boundary);
    let _contained = tripled_or_error(boundary);
    let integer_total = sum_integers([1, -2, 4]);
    let float_total = sum_floats([1.5, -2.25, 3.0]);
    let empty_totals = sum_integers([]) == 0 and sum_floats([]) == 0.0;
    dispose(boundary);
    observed == 42 and copied.value == observed and owned.value == 7 and owned.doubled == 14 and copied_label == "42" and copied_bytes.length() == 2 and copied_samples == [42, 84] and integer_total == 3 and float_total == 2.25 and empty_totals and (copied_mode == Raw or copied_mode == Doubled or copied_mode == Offset) and live_count() == 0.

pub main(): Unit ->
    if {
        valid_lifecycle() -> println("CPP_PACKAGE_CONSUMER_OK");
        true -> println("CPP_PACKAGE_CONSUMER_FAILED")
    }.
"#
}

/// Returns a consumer that attempts to read an explicitly disposed handle.
fn stale_consumer_source() -> &'static str {
    r#"module cpp_consumer.Main.

import cpp_fixture.NativeBoundary.{dispose, new, value}.
import std.io.Console.{println}.

pub main(): Unit ->
    let boundary = new(7);
    let stale = boundary;
    dispose(boundary);
    let observed = value(stale);
    println("STALE_HANDLE_NOT_REJECTED").
"#
}

/// Requires command success and prints both streams in failure diagnostics.
fn assert_success(output: Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed:\n{}",
        render_output(&output)
    );
}

/// Renders captured command streams for stable test diagnostics.
fn render_output(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
