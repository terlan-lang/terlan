#![allow(dead_code)]

use serde::Serialize;

use crate::backends::wasm::{
    emit_module, lower_core_module, wasm_abi_contract_checksum, wasm_abi_signature_checksum,
    wasm_checksum, WasmAbiSignature, WasmEmitError, WasmFunction, WasmModuleIr, WasmResultType,
};
use crate::terlan_typeck::CoreModule;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};

use super::{write_build_file, BuildOneError};

const WASM_CORE_ARTIFACT_SCHEMA_VERSION: &str = "terlan-wasm-core-artifact-v0";
const WASM_CORE_ARTIFACT_KIND: &str = "terlan-wasm-core";
const WASM_CORE_TARGET_PROFILE: &str = "wasm.core";
const WASM_CORE_VALIDATION_ENGINE: &str = "wasmparser";

/// Build-ready Wasm core artifact.
///
/// Inputs:
/// - Checked CoreIR accepted by the Wasm backend subset.
///
/// Output:
/// - Validated WebAssembly bytes and JSON-ready manifest metadata.
///
/// Transformation:
/// - Keeps package artifact emission separate from public target promotion so
///   the compiler can validate deterministic Wasm output before enabling the
///   `wasm-core` manifest artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WasmCoreBuildArtifact {
    pub(super) wasm_bytes: Vec<u8>,
    pub(super) manifest: WasmCoreBuildManifest,
}

/// JSON-ready manifest for a Wasm core artifact.
///
/// Inputs:
/// - Lowered Wasm backend IR and validated byte payload.
///
/// Output:
/// - Stable metadata for package manifests, release checks, and future CLI
///   artifact emission.
///
/// Transformation:
/// - Records exported function ABI, compiler version, validator, and byte
///   checksum without depending on shell tools or external Wasm runtimes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WasmCoreBuildManifest {
    pub(super) schema_version: &'static str,
    pub(super) artifact_kind: &'static str,
    pub(super) compiler_version: &'static str,
    pub(super) target_profile: &'static str,
    pub(super) module: String,
    pub(super) exports: Vec<WasmCoreExportManifest>,
    pub(super) validation_engine: &'static str,
    pub(super) abi_contract_checksum: String,
    pub(super) signature_checksum: String,
    pub(super) checksum: String,
}

/// JSON-ready export metadata for one Wasm function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WasmCoreExportManifest {
    pub(super) name: String,
    pub(super) params: Vec<WasmCoreParamManifest>,
    pub(super) result: &'static str,
}

/// JSON-ready parameter metadata for one Wasm function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct WasmCoreParamManifest {
    pub(super) name: String,
    pub(super) ty: &'static str,
}

/// Wasm artifact planning error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WasmCoreBuildError {
    Lower(String),
    Emit(String),
}

impl std::fmt::Display for WasmCoreBuildError {
    /// Formats a Wasm artifact planning error.
    ///
    /// Inputs: formatter sink.
    /// Output: formatting result.
    /// Transformation: maps lower/emission failures into stable diagnostics.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lower(message) => write!(f, "Wasm CoreIR lowering failed: {message}"),
            Self::Emit(message) => write!(f, "Wasm artifact emission failed: {message}"),
        }
    }
}

impl std::error::Error for WasmCoreBuildError {}

impl From<crate::backends::wasm::WasmLowerError> for WasmCoreBuildError {
    /// Converts a backend lowering error into the command artifact error.
    ///
    /// Inputs: backend lowering error.
    /// Output: command-layer artifact error.
    /// Transformation: preserves the backend diagnostic text for callers.
    fn from(error: crate::backends::wasm::WasmLowerError) -> Self {
        Self::Lower(error.to_string())
    }
}

impl From<WasmEmitError> for WasmCoreBuildError {
    /// Converts a backend emission error into the command artifact error.
    ///
    /// Inputs: backend emission error.
    /// Output: command-layer artifact error.
    /// Transformation: preserves the backend diagnostic text for callers.
    fn from(error: WasmEmitError) -> Self {
        Self::Emit(error.to_string())
    }
}

/// Builds a deterministic Wasm artifact from checked CoreIR.
///
/// Inputs:
/// - `core`: checked CoreIR module from the formal compiler pipeline.
///
/// Output:
/// - Validated Wasm bytes and manifest metadata, or a command-layer diagnostic.
///
/// Transformation:
/// - Lowers CoreIR through the backend-owned Wasm lowerer, emits bytes through
///   `wasm-encoder`, validates with `wasmparser`, and fingerprints the emitted
///   bytes for deterministic package metadata.
pub(super) fn build_wasm_core_artifact(
    core: &CoreModule,
) -> Result<WasmCoreBuildArtifact, WasmCoreBuildError> {
    let module_ir = lower_core_module(core)?;
    let wasm_bytes = emit_module(&module_ir)?;
    let manifest = build_manifest(core, &module_ir, &wasm_bytes);
    Ok(WasmCoreBuildArtifact {
        wasm_bytes,
        manifest,
    })
}

/// Emits one already-checked CoreIR module for command-owned Wasm execution.
pub(crate) fn write_checked_wasm_core_artifact(
    core: &CoreModule,
    state: &crate::CliState,
) -> Result<std::path::PathBuf, String> {
    let artifact = build_wasm_core_artifact(core).map_err(|error| error.to_string())?;
    write_wasm_core_artifact(core, &artifact, state).map_err(|error| match error {
        BuildOneError::Message(message) => message,
        BuildOneError::Exit(code) => format!("Wasm artifact emission exited with {code:?}"),
    })?;
    Ok(state
        .out_dir
        .join("wasm")
        .join(format!("{}.wasm", module_file_stem(core))))
}

/// Builds and writes one Wasm core artifact from one source file.
///
/// Inputs:
/// - `path`: Terlan source path.
/// - `state`: global CLI state carrying output and diagnostic settings.
///
/// Output:
/// - `Ok(())` after `.wasm` bytes and manifest metadata are written, or a
///   build error.
///
/// Transformation:
/// - Runs the formal compiler pipeline, lowers checked CoreIR through the
///   Wasm backend, validates emitted bytes, and writes both binary and JSON
///   manifest artifacts under `_build/wasm`.
pub(super) fn build_one_wasm_core_artifact(
    path: &str,
    state: &crate::CliState,
) -> Result<(), BuildOneError> {
    let source = crate::support::read_file(path).map_err(BuildOneError::Message)?;
    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            TargetProfile::WasmCore,
            TargetProfileCheckOptions {
                allow_asset_imports: false,
                allow_rust_backed_std_modules: false,
            },
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return Err(BuildOneError::Exit(exit_code)),
        };
    let artifact = build_wasm_core_artifact(&compiled.core)
        .map_err(|err| BuildOneError::Message(err.to_string()))?;

    if state.no_emit {
        return Ok(());
    }

    write_wasm_core_artifact(&compiled.core, &artifact, state)
}

/// Builds manifest metadata for a Wasm artifact.
///
/// Inputs:
/// - Checked CoreIR module.
/// - Lowered Wasm module IR.
/// - Validated Wasm bytes.
///
/// Output:
/// - JSON-ready manifest metadata.
///
/// Transformation:
/// - Projects exported functions into stable ABI metadata and records a
///   deterministic compiler fingerprint over the exact byte payload.
fn build_manifest(
    core: &CoreModule,
    module_ir: &WasmModuleIr,
    wasm_bytes: &[u8],
) -> WasmCoreBuildManifest {
    let exports = module_ir
        .functions
        .iter()
        .filter_map(export_manifest)
        .collect::<Vec<_>>();
    let signatures = exports
        .iter()
        .map(|export| WasmAbiSignature {
            name: export.name.clone(),
            params: export
                .params
                .iter()
                .map(|param| param.ty.to_string())
                .collect(),
            result: export.result.to_string(),
        })
        .collect::<Vec<_>>();
    WasmCoreBuildManifest {
        schema_version: WASM_CORE_ARTIFACT_SCHEMA_VERSION,
        artifact_kind: WASM_CORE_ARTIFACT_KIND,
        compiler_version: env!("CARGO_PKG_VERSION"),
        target_profile: WASM_CORE_TARGET_PROFILE,
        module: core.module.clone(),
        exports,
        validation_engine: WASM_CORE_VALIDATION_ENGINE,
        abi_contract_checksum: wasm_abi_contract_checksum(),
        signature_checksum: wasm_abi_signature_checksum(&signatures),
        checksum: wasm_artifact_checksum(wasm_bytes),
    }
}

/// Builds manifest metadata for one exported Wasm function.
///
/// Inputs:
/// - `function`: lowered backend function.
///
/// Output:
/// - Export manifest entry when the function is exported.
///
/// Transformation:
/// - Converts typed backend ABI values into stable manifest strings.
fn export_manifest(function: &WasmFunction) -> Option<WasmCoreExportManifest> {
    let export = function.export.as_ref()?;
    Some(WasmCoreExportManifest {
        name: export.name.clone(),
        params: function
            .params
            .iter()
            .map(|param| WasmCoreParamManifest {
                name: param.name.clone(),
                ty: wasm_type_name(param.ty),
            })
            .collect(),
        result: wasm_type_name(function.result),
    })
}

/// Names a Wasm backend scalar type for manifest output.
///
/// Inputs: Wasm backend scalar type.
/// Output: stable manifest spelling.
/// Transformation: maps the first supported scalar ABI to its W3C type name.
fn wasm_type_name(ty: WasmResultType) -> &'static str {
    match ty {
        WasmResultType::I32 => "i32",
        WasmResultType::I64 => "i64",
        WasmResultType::F32 => "f32",
        WasmResultType::F64 => "f64",
    }
}

/// Computes a deterministic artifact checksum for Wasm bytes.
///
/// Inputs:
/// - `wasm_bytes`: validated WebAssembly byte payload.
///
/// Output:
/// - Stable FNV-1a checksum string matching other compiler artifact metadata.
///
/// Transformation:
/// - Hashes exact bytes without invoking external checksum programs.
fn wasm_artifact_checksum(wasm_bytes: &[u8]) -> String {
    wasm_checksum(wasm_bytes)
}

/// Writes Wasm bytes and manifest metadata for one CoreIR module.
///
/// Inputs:
/// - `core`: checked CoreIR module used for deterministic file naming.
/// - `artifact`: build-ready Wasm artifact payload.
/// - `state`: CLI state containing output directory and incremental mode.
///
/// Output:
/// - Success after both files are written.
///
/// Transformation:
/// - Creates `_build/wasm`, writes `<module>.wasm`, and writes
///   `<module>.wasm.json` with pretty JSON manifest metadata.
fn write_wasm_core_artifact(
    core: &CoreModule,
    artifact: &WasmCoreBuildArtifact,
    state: &crate::CliState,
) -> Result<(), BuildOneError> {
    let wasm_dir = state.out_dir.join("wasm");
    std::fs::create_dir_all(&wasm_dir).map_err(|err| {
        BuildOneError::Message(format!("cannot create Wasm artifact directory: {err}"))
    })?;
    let stem = module_file_stem(core);
    let wasm_path = wasm_dir.join(format!("{stem}.wasm"));
    let manifest_path = wasm_dir.join(format!("{stem}.wasm.json"));
    write_build_file(&wasm_path, &artifact.wasm_bytes, state.incremental)
        .map_err(BuildOneError::Message)?;
    let manifest_json = serde_json::to_string_pretty(&artifact.manifest).map_err(|err| {
        BuildOneError::Message(format!("failed to serialize Wasm artifact manifest: {err}"))
    })?;
    write_build_file(
        &manifest_path,
        format!("{manifest_json}\n").as_bytes(),
        state.incremental,
    )
    .map_err(BuildOneError::Message)
}

/// Returns the filesystem stem for a Wasm artifact.
///
/// Inputs:
/// - `core`: checked CoreIR module.
///
/// Output:
/// - Stable filename stem derived from the module name.
///
/// Transformation:
/// - Replaces module path separators with underscores so the artifact remains
///   portable across filesystems.
fn module_file_stem(core: &CoreModule) -> String {
    core.module.replace('.', "_")
}

#[cfg(test)]
#[path = "wasm_artifact_test.rs"]
mod wasm_artifact_test;
