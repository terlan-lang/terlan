use super::*;
use crate::backends::wasm::validate_module;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

/// Compiles a Terlan fixture into checked CoreIR.
///
/// Inputs:
/// - `source`: Terlan module source.
///
/// Output:
/// - Checked CoreIR module produced by the production formal pipeline.
///
/// Transformation:
/// - Keeps Wasm artifact tests on the same frontend and type-checking path as
///   normal compiler builds.
fn compile_core(source: &str) -> CoreModule {
    crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "wasm_artifact_test.terl",
        source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::default(),
    )
    .expect("compile Wasm artifact fixture")
    .core
}

/// Verifies checked CoreIR produces validated Wasm bytes and manifest metadata.
///
/// Inputs:
/// - A Terlan module exporting `Int` and `Bool` functions in the supported
///   Wasm subset.
///
/// Output:
/// - Valid Wasm bytes plus deterministic manifest metadata.
///
/// Transformation:
/// - Exercises the command-owned artifact envelope while reusing backend-owned
///   lowering, emission, and validation.
#[test]
fn wasm_core_artifact_emits_valid_bytes_and_manifest() {
    let core = compile_core(
        r#"
module wasm.Build.

pub add(left: Int, right: Int): Int ->
    left + right.

pub less(left: Int, right: Int): Bool ->
    left < right.
"#,
    );

    let artifact = build_wasm_core_artifact(&core).expect("build Wasm artifact");

    validate_module(&artifact.wasm_bytes).expect("artifact bytes must validate");
    assert_eq!(
        artifact.manifest.schema_version,
        "terlan-wasm-core-artifact-v0"
    );
    assert_eq!(artifact.manifest.artifact_kind, "terlan-wasm-core");
    assert_eq!(artifact.manifest.target_profile, "wasm.core");
    assert_eq!(artifact.manifest.module, "wasm.Build");
    assert_eq!(artifact.manifest.validation_engine, "wasmparser");
    assert_eq!(
        artifact.manifest.abi_contract_checksum,
        wasm_abi_contract_checksum()
    );
    assert!(artifact.manifest.signature_checksum.starts_with("fnv1a64:"));
    assert!(artifact.manifest.checksum.starts_with("fnv1a64:"));
    assert_eq!(
        artifact.manifest.checksum.len(),
        "fnv1a64:0000000000000000".len()
    );

    assert_eq!(artifact.manifest.exports.len(), 2);
    assert_eq!(artifact.manifest.exports[0].name, "add");
    assert_eq!(
        artifact.manifest.exports[0].params,
        vec![
            WasmCoreParamManifest {
                name: "left".to_string(),
                ty: "i32",
            },
            WasmCoreParamManifest {
                name: "right".to_string(),
                ty: "i32",
            },
        ]
    );
    assert_eq!(artifact.manifest.exports[0].result, "i32");
    assert_eq!(artifact.manifest.exports[1].name, "less");
    assert_eq!(artifact.manifest.exports[1].result, "i32");
}

/// Verifies Wasm artifact generation is deterministic for identical CoreIR.
///
/// Inputs:
/// - The same checked CoreIR module built twice.
///
/// Output:
/// - Equal Wasm bytes and equal manifest metadata.
///
/// Transformation:
/// - Locks the artifact envelope against nondeterministic export ordering,
///   validation metadata, or checksum generation.
#[test]
fn wasm_core_artifact_is_deterministic() {
    let core = compile_core(
        r#"
module wasm.Deterministic.

pub answer(): Int ->
    40 + 2.
"#,
    );

    let first = build_wasm_core_artifact(&core).expect("build first Wasm artifact");
    let second = build_wasm_core_artifact(&core).expect("build second Wasm artifact");

    assert_eq!(first, second);
}

/// Verifies unsupported CoreIR fails before artifact metadata is emitted.
///
/// Inputs:
/// - A checked Terlan module whose public function uses an unsupported body
///   form for the first Wasm subset.
///
/// Output:
/// - Command-layer lowering diagnostic.
///
/// Transformation:
/// - Prevents the build artifact layer from pretending unsupported CoreIR has
///   a valid Wasm byte or manifest representation.
#[test]
fn wasm_core_artifact_rejects_unsupported_coreir_body() {
    let core = compile_core(
        r#"
module wasm.UnsupportedArtifact.

pub answer(): Int ->
    let value = 42;
    value.
"#,
    );

    let err = build_wasm_core_artifact(&core).expect_err("let body is not implemented yet");

    assert!(err.to_string().contains("Wasm CoreIR lowering failed"));
    assert!(err
        .to_string()
        .contains("supports only i32 literals, locals, arithmetic, and comparison expressions"));
    assert!(err.to_string().contains("`answer` body is Let"));
}
