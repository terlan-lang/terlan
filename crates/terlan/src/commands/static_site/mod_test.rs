use super::command::static_check_args;
use super::*;
use std::time::UNIX_EPOCH;

/// Verifies static check arguments force one-shot validation mode.
///
/// Inputs:
/// - Command-local args after `terlc static check`.
///
/// Output:
/// - Args accepted by the underlying static serve runner.
///
/// Transformation:
/// - Preserves the source file and appends `--check` plus
///   `--validate-output` so the public check command cannot bind a server.
#[test]
fn static_check_args_adds_check_and_validation_flags() {
    assert_eq!(
        static_check_args(&["src/site/Site.terl".to_string()]),
        vec![
            "src/site/Site.terl".to_string(),
            "--check".to_string(),
            "--validate-output".to_string()
        ]
    );
}

/// Verifies static check argument construction does not duplicate flags.
///
/// Inputs:
/// - Command-local args that already include static check flags.
///
/// Output:
/// - The original argument vector without duplicate flag entries.
///
/// Transformation:
/// - Confirms public wrapper construction remains deterministic when users
///   pass explicit validation flags.
#[test]
fn static_check_args_preserves_existing_flags() {
    assert_eq!(
        static_check_args(&[
            "src/site/Site.terl".to_string(),
            "--validate-output".to_string(),
            "--check".to_string()
        ]),
        vec![
            "src/site/Site.terl".to_string(),
            "--validate-output".to_string(),
            "--check".to_string()
        ]
    );
}

#[test]
fn static_preview_requires_docs_mode() {
    let emit_error =
        parse_emit_static_args(&["src/site/Site.terl".to_string(), "--preview".to_string()])
            .expect_err("emit preview without docs should fail");
    assert_eq!(emit_error, "--preview requires --docs");

    let serve_error =
        parse_serve_static_args(&["src/site/Site.terl".to_string(), "--preview".to_string()])
            .expect_err("serve preview without docs should fail");
    assert_eq!(serve_error, "--preview requires --docs");
}

#[test]
fn static_production_docs_require_valid_explicit_cutoff() {
    let missing = parse_emit_static_args(&["src/site/Site.terl".to_string(), "--docs".to_string()])
        .expect_err("production docs without cutoff should fail");
    assert!(missing.contains("require --as-of YYYY-MM-DD"));

    let invalid = parse_emit_static_args(&[
        "src/site/Site.terl".to_string(),
        "--docs".to_string(),
        "--as-of".to_string(),
        "2026-02-30".to_string(),
    ])
    .expect_err("invalid cutoff should fail");
    assert!(invalid.contains("invalid publication cutoff"));

    let conflicting = parse_emit_static_args(&[
        "src/site/Site.terl".to_string(),
        "--docs".to_string(),
        "--preview".to_string(),
        "--as-of".to_string(),
        "2026-08-16".to_string(),
    ])
    .expect_err("preview with cutoff should fail");
    assert_eq!(conflicting, "--as-of cannot be combined with --preview");
}

/// Verifies the public static check wrapper renders once and exits.
///
/// Inputs:
/// - A temporary static Terlan source file.
/// - `terlc static check` command-local arguments.
///
/// Output:
/// - Success exit code and generated HTML in the configured output directory.
///
/// Transformation:
/// - Exercises the public command-group adapter instead of calling the hidden
///   static serve runner directly.
#[test]
fn public_static_check_renders_once_and_exits() {
    let dir = temp_static_dir("public_static_check");
    let source_path = dir.join("Site.terl");
    let out_dir = dir.join("public");
    fs::write(
        &source_path,
        "module site.\n\npub page(): Html ->\n    html { <main>Check</main> }.\n",
    )
    .expect("write static source");

    let exit = run(
        CliCommand {
            verb: Some("static".to_string()),
            args: vec!["check".to_string(), source_path.display().to_string()],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read generated html"),
        "<main>Check</main>"
    );
    fs::remove_dir_all(dir).expect("cleanup static check fixture");
}

/// Creates an isolated temporary directory for static-site tests.
///
/// Inputs:
/// - `name`: human-readable test case prefix.
///
/// Output:
/// - Filesystem path for a newly created temporary directory.
///
/// Transformation:
/// - Combines process id and nanosecond timestamp so parallel tests do not
///   collide.
fn temp_static_dir(name: &str) -> PathBuf {
    let nonce = UNIX_EPOCH
        .elapsed()
        .expect("system clock after Unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "terlan_static_site_{name}_{}_{}",
        std::process::id(),
        nonce
    ));
    fs::create_dir_all(&dir).expect("create static test dir");
    dir
}
