use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat, DocFormat};

/// Verifies the experimental Rust VM can execute a Terlan hello-world module.
///
/// Inputs:
/// - Temporary Terlan source importing `std.io.Console.println`.
///
/// Output:
/// - Captured VM output and `Unit` return value.
///
/// Transformation:
/// - Compiles through the normal frontend, loads CoreIR into `TerlanVm`, and
///   executes `main/0` without generated Erlang or BEAM.
#[test]
fn vm_run_loads_hello_world_source_and_executes_main() {
    let root = unique_temp_dir("terlan-vm-hello");
    let source = root.join("Main.terl");
    fs::create_dir_all(&root).expect("create temp dir");
    fs::write(
        &source,
        "module vm_hello.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"hello from Rust VM\").\n",
    )
    .expect("write source");
    let mut lines = Vec::new();
    let mut output = |line: &str| lines.push(line.to_string());

    let value =
        run_source_file_in_vm(&source, "main", &test_state(), &mut output).expect("run in VM");

    assert_eq!(value, ReplValue::Unit);
    assert_eq!(lines, vec!["hello from Rust VM".to_string()]);
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies the Rust VM reports missing entrypoints before any backend run.
///
/// Inputs:
/// - Temporary source with `main/0`.
/// - Explicit missing entry function name.
///
/// Output:
/// - Stable VM error mentioning the missing function.
///
/// Transformation:
/// - Proves VM execution errors come from the Rust CoreIR runtime, not an
///   Erlang `{undef, ...}` failure.
#[test]
fn vm_run_reports_missing_entrypoint_as_vm_error() {
    let root = unique_temp_dir("terlan-vm-missing-entry");
    let source = root.join("Main.terl");
    fs::create_dir_all(&root).expect("create temp dir");
    fs::write(
        &source,
        "module vm_hello.Main.\n\npub main(): Unit ->\n    Unit.\n",
    )
    .expect("write source");
    let mut output = |_line: &str| {};

    let error =
        run_source_file_in_vm(&source, "missing", &test_state(), &mut output).expect_err("error");

    assert!(
        error.contains("missing REPL function missing/0 in CoreIR"),
        "unexpected VM error: {error}"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

fn test_state() -> CliState {
    CliState {
        no_emit: false,
        incremental: false,
        timings: false,
        experimental: true,
        out_dir: PathBuf::from("_build"),
        cache_dir: None,
        trace_invalidation: false,
        diagnostic_format: DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        doc_format: DocFormat::Html,
        native_policy: NativePolicy::SafeNativeOptional,
        target_profile: TargetProfile::Erlang,
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}
