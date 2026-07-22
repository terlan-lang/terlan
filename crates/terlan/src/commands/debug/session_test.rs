use std::fs;
use std::path::PathBuf;

use crate::runtime::native_image::debug::TvmNativeDebugRecord;

use super::{resolve_breakpoint, source_line_span, source_path_matches};

/// Creates one source record suitable for breakpoint-resolution tests.
fn record(source_file: String) -> TvmNativeDebugRecord {
    TvmNativeDebugRecord {
        source_file,
        module: "app.Main".to_string(),
        function: "main".to_string(),
        arity: 0,
        span_start: 17,
        span_end: 43,
        core_schema: "terlan-core-v1".to_string(),
        proof_readiness: "Ready".to_string(),
    }
}

/// Allocates a deterministic process-local debugger fixture path.
fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "terlan-debug-session-{}-{name}.terl",
        std::process::id()
    ))
}

#[test]
fn module_breakpoint_resolves_embedded_function_identity() {
    let records = [record("src/app/Main.terl".to_string())];
    let resolution =
        resolve_breakpoint("app.Main.main", &records).expect("module breakpoint should resolve");

    assert_eq!(resolution.spec, "app.Main.main");
    assert_eq!(
        resolution.functions,
        ["app.Main.main/0@src/app/Main.terl:17..43"]
    );
}

#[test]
fn conditional_breakpoint_resolves_using_its_static_location() {
    let records = [record("src/app/Main.terl".to_string())];
    let resolution = resolve_breakpoint("app.Main.main where value == 1", &records)
        .expect("conditional breakpoint location should resolve");

    assert_eq!(resolution.functions.len(), 1);
}

#[test]
fn file_breakpoint_resolves_workspace_relative_source_line() {
    let path = fixture_path("file-line");
    let source = "module app.Main.\n\npub main(): Int ->\n    42.\n";
    fs::write(&path, source).expect("write debugger source fixture");
    let mut source_record = record(path.display().to_string());
    source_record.span_start = source.find("pub main").expect("declaration start");
    source_record.span_end = source.len();

    let spec = format!("{}:3", path.file_name().unwrap().to_string_lossy());
    let resolution = resolve_breakpoint(&spec, &[source_record])
        .expect("file breakpoint should resolve from suffix path");

    assert_eq!(resolution.functions.len(), 1);
    let _ = fs::remove_file(path);
}

#[test]
fn unresolved_breakpoint_has_stable_diagnostic() {
    let error = resolve_breakpoint("app.Main.missing", &[])
        .expect_err("unknown function should not resolve");

    assert_eq!(error.code, "debug_breakpoint_unresolved");
    assert!(error.message.contains("app.Main.missing"));
}

#[test]
fn source_line_span_rejects_invalid_and_non_boundary_ranges() {
    assert_eq!(source_line_span("one\ntwo\n", 0, 7), Some((1, 2)));
    assert_eq!(source_line_span("é\n", 0, 2), Some((1, 1)));
    assert_eq!(source_line_span("text", 2, 2), None);
    assert_eq!(source_line_span("text", 0, 10), None);
    assert_eq!(source_line_span("é", 1, 2), None);
}

#[test]
fn source_path_matching_accepts_exact_and_workspace_suffixes() {
    let absolute = PathBuf::from("/workspace/src/app/Main.terl");

    assert!(source_path_matches(&absolute, &absolute));
    assert!(source_path_matches(
        &absolute,
        &PathBuf::from("src/app/Main.terl")
    ));
    assert!(!source_path_matches(
        &absolute,
        &PathBuf::from("src/other/Main.terl")
    ));
}
