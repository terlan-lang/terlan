use super::*;
use std::path::PathBuf;

/// Builds the canonical public `I32` add fixture through the Wasm CLI target.
///
/// Inputs:
/// - `name`: unique temporary-workspace suffix for the owning test.
///
/// Output:
/// - Path to the emitted `.wasm` artifact.
///
/// Transformation:
/// - Writes one source fixture and drives the same command path used by users,
///   keeping artifact-validation and runtime-execution tests on identical input.
fn build_i32_add_fixture(name: &str) -> PathBuf {
    let dir = make_temp_dir(name);
    let source = dir.join("Main.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source,
        "module app.Main.\n\nimport std.wasm.Abi.{I32}.\n\npub add(left: I32, right: I32): I32 ->\n    left + right.\n\npub less(left: I32, right: I32): Bool ->\n    left < right.\n",
    )
    .expect("write source fixture");

    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source.display().to_string(),
            "--target".to_string(),
            "wasm.core".to_string(),
        ],
    };
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    out_dir.join("wasm/app_Main.wasm")
}

/// Verifies explicit `--target wasm.core` emits Wasm bytes and metadata.
///
/// Inputs:
/// - `terlc build --target wasm.core` over a pure integer source file.
///
/// Output:
/// - Validated `.wasm` bytes and `.wasm.json` manifest under `_build/wasm`.
///
/// Transformation:
/// - Exercises explicit CLI target promotion without requiring a project
///   manifest.
#[test]
fn build_command_emits_wasm_core_target_for_single_file() {
    let dir = make_temp_dir("build_command_wasm_core_single_file");
    let source = dir.join("Main.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source,
        "module app.Main.\n\npub add(left: Int, right: Int): Int ->\n    left + right.\n",
    )
    .expect("write source fixture");

    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source.display().to_string(),
            "--target".to_string(),
            "wasm.core".to_string(),
        ],
    };
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let wasm_path = out_dir.join("wasm/app_Main.wasm");
    let manifest_path = out_dir.join("wasm/app_Main.wasm.json");
    let wasm_bytes = fs::read(&wasm_path).expect("read emitted wasm bytes");
    crate::backends::wasm::validate_module(&wasm_bytes).expect("validate emitted wasm bytes");
    let manifest_text = fs::read_to_string(&manifest_path).expect("read emitted wasm manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse emitted wasm manifest");
    assert_eq!(manifest["target_profile"], "wasm.core");
    assert_eq!(manifest["module"], "app.Main");
    assert_eq!(manifest["exports"][0]["name"], "add");
}

/// Verifies explicit `--target wasm.core` accepts the `std.wasm.Abi.I32` alias.
///
/// Inputs:
/// - `terlc build --target wasm.core` over a source file importing `I32`.
///
/// Output:
/// - Validated `.wasm` bytes and manifest `i32` parameter/result metadata.
///
/// Transformation:
/// - Exercises the public source spelling for Wasm ABI types through command
///   dispatch, formal target-profile validation, backend lowering, and
///   artifact writing.
#[test]
fn build_command_emits_wasm_core_target_for_i32_abi_alias() {
    let wasm_path = build_i32_add_fixture("build_command_wasm_core_i32_abi_alias");
    let manifest_path = wasm_path.with_extension("wasm.json");
    let wasm_bytes = fs::read(&wasm_path).expect("read emitted wasm bytes");
    crate::backends::wasm::validate_module(&wasm_bytes).expect("validate emitted wasm bytes");
    let manifest_text = fs::read_to_string(&manifest_path).expect("read emitted wasm manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse emitted wasm manifest");
    assert_eq!(manifest["target_profile"], "wasm.core");
    assert_eq!(manifest["exports"][0]["params"][0]["ty"], "i32");
    assert_eq!(manifest["exports"][0]["result"], "i32");
}

/// Verifies a generated Wasm export executes through `terlc run`.
///
/// Inputs:
/// - A Terlan function over the public `std.wasm.Abi.I32` source type.
/// - The `.wasm` artifact emitted by `terlc build --target wasm.core`.
///
/// Output:
/// - A successful hosted WebAssembly invocation returning the expected `i32`.
///
/// Transformation:
/// - Exercises source parsing, CoreIR lowering, Wasm emission, artifact writing,
///   sidecar validation, runtime module validation, export resolution, argument
///   passing, and result decoding without a source-checkout adapter.
#[test]
fn build_command_executes_wasm_core_i32_export_with_run_command() {
    let wasm_path = build_i32_add_fixture("build_command_wasm_core_i32_runtime");
    let command = CliCommand {
        verb: Some("run".to_string()),
        args: vec![
            wasm_path.display().to_string(),
            "--export".to_string(),
            "add".to_string(),
            "--arg".to_string(),
            "i32:19".to_string(),
            "--arg".to_string(),
            "i32:23".to_string(),
            "--expect".to_string(),
            "i32:42".to_string(),
            "--repeat".to_string(),
            "3".to_string(),
        ],
    };

    assert_eq!(
        crate::commands::run::run(command, CliState::default()),
        ExitCode::SUCCESS
    );

    let comparison = CliCommand {
        verb: Some("run".to_string()),
        args: vec![
            wasm_path.display().to_string(),
            "--export".to_string(),
            "less".to_string(),
            "--arg".to_string(),
            "i32:19".to_string(),
            "--arg".to_string(),
            "i32:23".to_string(),
            "--expect".to_string(),
            "i32:1".to_string(),
            "--repeat".to_string(),
            "3".to_string(),
        ],
    };
    assert_eq!(
        crate::commands::run::run(comparison, CliState::default()),
        ExitCode::SUCCESS
    );
}

#[test]
fn build_command_emits_and_executes_all_wasm_scalar_abi_aliases() {
    let dir = make_temp_dir("build_command_wasm_scalar_abi");
    let source = dir.join("Scalar.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source,
        "module app.Scalar.\n\nimport std.wasm.Abi.{F32, F64, I32, I64}.\n\npub identity_i32(value: I32): I32 -> value.\npub identity_i64(value: I64): I64 -> value.\npub identity_f32(value: F32): F32 -> value.\npub identity_f64(value: F64): F64 -> value.\n",
    )
    .expect("write scalar ABI source");
    let command = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source.display().to_string()],
    };
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };

    assert_eq!(run(command, state), ExitCode::SUCCESS);
    let wasm_path = out_dir.join("wasm/app_Scalar.wasm");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(wasm_path.with_extension("wasm.json")).expect("read scalar manifest"),
    )
    .expect("parse scalar manifest");
    let signatures = manifest["exports"]
        .as_array()
        .expect("exports")
        .iter()
        .map(|export| {
            (
                export["name"].as_str().expect("name"),
                export["params"][0]["ty"].as_str().expect("param type"),
                export["result"].as_str().expect("result type"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        signatures,
        vec![
            ("identity_f32", "f32", "f32"),
            ("identity_f64", "f64", "f64"),
            ("identity_i32", "i32", "i32"),
            ("identity_i64", "i64", "i64"),
        ]
    );

    for (export, scalar) in [
        ("identity_i32", "i32:42"),
        ("identity_i64", "i64:9007199254740991"),
        ("identity_f32", "f32:1.5"),
        ("identity_f64", "f64:2.25"),
    ] {
        let command = CliCommand {
            verb: Some("run".to_string()),
            args: vec![
                wasm_path.display().to_string(),
                "--export".to_string(),
                export.to_string(),
                "--arg".to_string(),
                scalar.to_string(),
                "--expect".to_string(),
                scalar.to_string(),
            ],
        };
        assert_eq!(
            crate::commands::run::run(command, CliState::default()),
            ExitCode::SUCCESS,
            "failed scalar export {export}"
        );
    }
}

/// Verifies `std.wasm.Abi` import evidence selects the Wasm build path.
///
/// Inputs:
/// - `terlc build` without `--target` over a source file importing `I32`.
///
/// Output:
/// - Wasm artifacts are emitted and VM artifacts are not emitted.
///
/// Transformation:
/// - Locks target inference to typed import evidence so users do not need a
///   redundant CLI target when source signatures already require Wasm ABI
///   types.
#[test]
fn build_command_infers_wasm_core_target_from_i32_abi_import() {
    let dir = make_temp_dir("build_command_wasm_core_i32_inferred");
    let source = dir.join("Main.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source,
        "module app.Main.\n\nimport std.wasm.Abi.{I32}.\n\npub add(left: I32, right: I32): I32 ->\n    left + right.\n",
    )
    .expect("write source fixture");

    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source.display().to_string()],
    };
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    assert!(out_dir.join("wasm/app_Main.wasm").exists());
    assert!(out_dir.join("wasm/app_Main.wasm.json").exists());
    assert!(!out_dir.join("vm").exists());
}

/// Verifies reserved project Wasm artifacts stop before Vm build emission.
///
/// Inputs:
/// - Parsed manifest selecting `wasm-browser`.
///
/// Output:
/// - Test assertion only; artifact dispatch must produce a reserved-family
///   diagnostic instead of falling through to the Vm backend.
///
/// Transformation:
/// - Exercises project artifact dispatch without scanning source roots.
#[test]
fn wasm_build_target_rejects_reserved_wasm_project_artifact() {
    let manifest = project_manifest::parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasm-browser\"\n\n[target.wasm]\nprofile = \"browser\"\n",
        std::path::Path::new("terlan.toml"),
    )
    .expect("manifest should parse");

    let err = reserved_project_artifact_build_error(&manifest)
        .expect("wasm project artifact should be reserved");

    assert!(err.contains("artifact `wasm-browser`"));
    assert!(err.contains("reserved for the Wasm target family"));
}

/// Verifies reserved project WASI artifacts stop before Vm build emission.
///
/// Inputs:
/// - Parsed manifest selecting `wasi-http`.
///
/// Output:
/// - Test assertion only; artifact dispatch must produce a reserved-family
///   diagnostic instead of falling through to the Vm backend.
///
/// Transformation:
/// - Exercises project artifact dispatch without scanning source roots.
#[test]
fn wasm_build_target_rejects_reserved_wasi_project_artifact() {
    let manifest = project_manifest::parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasi-http\"\n\n[target.wasi]\nprofile = \"http\"\n",
        std::path::Path::new("terlan.toml"),
    )
    .expect("manifest should parse");

    let err = reserved_project_artifact_build_error(&manifest)
        .expect("wasi project artifact should be reserved");

    assert!(err.contains("artifact `wasi-http`"));
    assert!(err.contains("reserved for the WASI target family"));
}

/// Verifies manifest-backed `wasm-core` projects emit Wasm bytes and metadata.
///
/// Inputs:
/// - A project manifest selecting `[build] artifact = "wasm-core"`.
/// - A source-root module using the first supported pure integer subset.
///
/// Output:
/// - Validated `.wasm` bytes and `.wasm.json` manifest under `_build/wasm`.
///
/// Transformation:
/// - Exercises manifest build dispatch with the same artifact writer used by
///   the explicit CLI target.
#[test]
fn build_command_emits_wasm_core_project_artifact() {
    let dir = make_temp_dir("directory_project_wasm_core_artifact");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("create wasm project source dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"wasm-core\"\n\n[target.wasm]\nprofile = \"core\"\n",
    )
    .expect("write wasm-core project manifest");
    fs::write(
        app_dir.join("Math.terl"),
        "module app.Math.\n\npub add(left: Int, right: Int): Int ->\n    left + right.\n",
    )
    .expect("write wasm-core source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![project_dir.display().to_string()],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("vm").exists());
    let wasm_path = out_dir.join("wasm/app_Math.wasm");
    let manifest_path = out_dir.join("wasm/app_Math.wasm.json");
    let wasm_bytes = fs::read(&wasm_path).expect("read emitted wasm bytes");
    crate::backends::wasm::validate_module(&wasm_bytes).expect("validate emitted wasm bytes");
    let manifest_text = fs::read_to_string(&manifest_path).expect("read emitted wasm manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse emitted wasm manifest");
    assert_eq!(manifest["artifact_kind"], "terlan-wasm-core");
    assert_eq!(manifest["target_profile"], "wasm.core");
    assert_eq!(manifest["module"], "app.Math");
    assert_eq!(manifest["exports"][0]["name"], "add");
    assert_eq!(manifest["exports"][0]["params"][0]["ty"], "i32");
    assert_eq!(manifest["exports"][0]["result"], "i32");
    assert_eq!(manifest["validation_engine"], "wasmparser");
    assert!(manifest["abi_contract_checksum"]
        .as_str()
        .expect("ABI contract checksum")
        .starts_with("fnv1a64:"));
    assert!(manifest["signature_checksum"]
        .as_str()
        .expect("signature checksum")
        .starts_with("fnv1a64:"));
    assert!(manifest["checksum"]
        .as_str()
        .expect("checksum")
        .starts_with("fnv1a64:"));
}
