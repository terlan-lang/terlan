//! Full-cycle execution of a generated C++ binding as an external Git package.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// Proves generated C++ packages build and execute through public package commands.
#[test]
#[ignore = "requires built terlc and terlan-vm binaries; run make cpp-package-consumer-check"]
fn generated_cpp_git_package_executes_and_rejects_stale_handles() {
    let root = temporary_root();
    let package = root.join("native_boundary_package");
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
    generate_native_lockfile(&package);
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

    fs::remove_dir_all(root).expect("remove external package test workspace");
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
        .env_remove("TERLAN_NATIVE_BOUNDARY_HELPER_PATH")
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
    assert_eq!(rust["helper_env"], "TERLAN_NATIVE_BOUNDARY_HELPER_PATH");
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
    let contained = tripled_or_error(boundary);
    let integer_total = sum_integers([1, -2, 4]);
    let float_total = sum_floats([1.5, -2.25, 3.0]);
    let empty_totals = sum_integers([]) == 0 and sum_floats([]) == 0.0;
    dispose(boundary);
    observed == 42 and copied.value == observed and owned.value == 7 and owned.doubled == 14 and copied_label == "42" and copied_bytes.length() == 2 and copied_samples == [42, 84] and integer_total == 3 and float_total == 2.25 and empty_totals and (copied_mode == Raw or copied_mode == Doubled or copied_mode == Offset) and std.core.Result.is_ok(contained) and live_count() == 0.

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
