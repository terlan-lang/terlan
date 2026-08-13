use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::backends::wasm::{
    validate_module, wasm_abi_contract_checksum, wasm_abi_signature_checksum, wasm_checksum,
    WasmAbiSignature,
};
use crate::commands::process_runner::run_command_with_timeout;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 60_000;
const MAX_REPEAT: u32 = 100_000;
const MANIFEST_SCHEMA: &str = "terlan-wasm-core-artifact-v0";
const ARTIFACT_KIND: &str = "terlan-wasm-core";

const NODE_WASM_RUNNER: &str = r#"
const fs = require('node:fs');
const [artifactPath, exportName, encodedArgs, repeatText, encodedHosts] = process.argv.slice(1);
const fail = (family, detail) => {
  process.stderr.write(`${family}: ${detail}\n`);
  process.exit(1);
};
let module;
try {
  module = new WebAssembly.Module(fs.readFileSync(artifactPath));
} catch (error) {
  fail('wasm-runtime-trap', error.message);
}
const imports = WebAssembly.Module.imports(module);
const hostReturns = JSON.parse(encodedHosts);
const importObject = {};
for (const imported of imports) {
  if (imported.kind !== 'function') {
    fail('wasm-import-unsupported', `${imported.module}.${imported.name}:${imported.kind}`);
  }
  const host = hostReturns.find(({ module, name }) =>
    module === imported.module && name === imported.name
  );
  if (!host) {
    fail('wasm-import-missing', `${imported.module}.${imported.name}`);
  }
  importObject[imported.module] ??= {};
  importObject[imported.module][imported.name] = () =>
    host.value.kind === 'i64' ? BigInt(host.value.value) : Number(host.value.value);
}
const unsupportedExport = WebAssembly.Module.exports(module)
  .find(({ kind }) => kind !== 'function');
if (unsupportedExport) {
  fail('wasm-export-unsupported', `${unsupportedExport.name}:${unsupportedExport.kind}`);
}
let instance;
try {
  instance = new WebAssembly.Instance(module, importObject);
} catch (error) {
  fail('wasm-runtime-trap', error.message);
}
const fn = instance.exports[exportName];
if (typeof fn !== 'function') {
  fail('wasm-export-missing', exportName);
}
const args = JSON.parse(encodedArgs).map(({ kind, value }) => {
  if (kind === 'i64') return BigInt(value);
  return Number(value);
});
let result;
try {
  for (let index = 0; index < Number(repeatText); index += 1) {
    const next = fn(...args);
    if (index > 0 && !Object.is(next, result)) {
      fail('wasm-runtime-trap', 'export returned inconsistent repeated results');
    }
    result = next;
  }
} catch (error) {
  fail('wasm-runtime-trap', error.message);
}
process.stdout.write(typeof result === 'bigint' ? result.toString() : String(result));
"#;

#[derive(Debug, Clone, PartialEq)]
/// Validated command configuration for one WASM artifact execution.
struct WasmRunConfig {
    artifact: PathBuf,
    export: Option<String>,
    args: Vec<WasmScalarArg>,
    host_returns: Vec<WasmHostReturn>,
    expected: Option<WasmScalarArg>,
    repeat: u32,
    timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Typed scalar argument serialized for the maintained WASM host runner.
struct WasmScalarArg {
    kind: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Deterministic scalar result supplied for one imported host function.
struct WasmHostReturn {
    module: String,
    name: String,
    value: WasmScalarArg,
}

#[derive(Debug, Deserialize)]
/// Compiler-emitted sidecar manifest consumed before WASM execution.
struct WasmArtifactManifest {
    schema_version: String,
    artifact_kind: String,
    module: String,
    exports: Vec<WasmExportManifest>,
    abi_contract_checksum: String,
    signature_checksum: String,
    checksum: String,
}

#[derive(Debug, Deserialize)]
/// One exported function signature recorded in the artifact manifest.
struct WasmExportManifest {
    name: String,
    params: Vec<WasmParamManifest>,
    result: String,
}

#[derive(Debug, Deserialize)]
/// One typed parameter recorded in an exported WASM signature.
struct WasmParamManifest {
    ty: String,
}

/// Returns whether `terlc run` arguments select an emitted Wasm artifact.
pub(crate) fn is_wasm_artifact_run(args: &[String]) -> bool {
    args.first()
        .is_some_and(|path| Path::new(path).extension().is_some_and(|ext| ext == "wasm"))
}

/// Executes a compiler-emitted Wasm artifact through the maintained Node/V8 runtime.
pub(crate) fn run(args: &[String]) -> ExitCode {
    match parse_run_config(args).and_then(|config| execute(&config)) {
        Ok(result) => {
            println!("{result}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Parses and bounds command arguments for a WASM artifact run.
fn parse_run_config(args: &[String]) -> Result<WasmRunConfig, String> {
    let artifact = args
        .first()
        .filter(|value| !value.starts_with('-'))
        .map(PathBuf::from)
        .ok_or_else(|| "wasm-artifact-missing: terlc run requires a .wasm artifact".to_string())?;
    let mut export = None;
    let mut scalar_args = Vec::new();
    let mut host_returns = Vec::new();
    let mut expected = None;
    let mut repeat = 1;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut index = 1;
    while index < args.len() {
        let option = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("wasm-argument-invalid: missing value for `{option}`"))?;
        match option {
            "--export" => export = Some(non_empty(value, "export")?.to_string()),
            "--arg" => scalar_args.push(parse_scalar_arg(value)?),
            "--host-return" => host_returns.push(parse_host_return(value)?),
            "--expect" => expected = Some(parse_scalar_arg(value)?),
            "--repeat" => repeat = parse_bounded_u32(value, "repeat", MAX_REPEAT)?,
            "--timeout-ms" => {
                timeout_ms = u64::from(parse_bounded_u32(value, "timeout", MAX_TIMEOUT_MS as u32)?)
            }
            _ => {
                return Err(format!(
                    "wasm-argument-invalid: unsupported option `{option}`"
                ))
            }
        }
        index += 2;
    }
    Ok(WasmRunConfig {
        artifact,
        export,
        args: scalar_args,
        host_returns,
        expected,
        repeat,
        timeout: Duration::from_millis(timeout_ms),
    })
}

/// Requires a nonempty command argument while preserving its borrowed value.
fn non_empty<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    if value.trim().is_empty() {
        Err(format!("wasm-argument-invalid: {label} cannot be empty"))
    } else {
        Ok(value)
    }
}

/// Parses a positive integer command option bounded by the supplied maximum.
fn parse_bounded_u32(value: &str, label: &str, max: u32) -> Result<u32, String> {
    let parsed = value.parse::<u32>().map_err(|_| {
        format!("wasm-argument-invalid: {label} must be an integer between 1 and {max}")
    })?;
    if parsed == 0 || parsed > max {
        return Err(format!(
            "wasm-argument-invalid: {label} must be an integer between 1 and {max}"
        ));
    }
    Ok(parsed)
}

/// Parses one `type:value` WASM scalar argument.
fn parse_scalar_arg(value: &str) -> Result<WasmScalarArg, String> {
    let (kind, raw) = value.split_once(':').ok_or_else(|| {
        "wasm-argument-invalid: scalar arguments use i32:V, i64:V, f32:V, or f64:V".to_string()
    })?;
    match kind {
        "i32" => {
            raw.parse::<i32>().map_err(|_| invalid_scalar(value))?;
        }
        "i64" => {
            raw.parse::<i64>().map_err(|_| invalid_scalar(value))?;
        }
        "f32" => require_finite(
            raw.parse::<f32>().map_err(|_| invalid_scalar(value))?,
            value,
        )?,
        "f64" => require_finite(
            raw.parse::<f64>().map_err(|_| invalid_scalar(value))?,
            value,
        )?,
        _ => return Err(invalid_scalar(value)),
    }
    Ok(WasmScalarArg {
        kind: kind.to_string(),
        value: raw.to_string(),
    })
}

/// Parses one deterministic `module.name=type:value` host result.
fn parse_host_return(value: &str) -> Result<WasmHostReturn, String> {
    let (qualified_name, scalar) = value.split_once('=').ok_or_else(|| {
        "wasm-argument-invalid: host returns use module.name=type:value".to_string()
    })?;
    let (module, name) = qualified_name.rsplit_once('.').ok_or_else(|| {
        "wasm-argument-invalid: host returns use module.name=type:value".to_string()
    })?;
    let module = non_empty(module, "host module")?;
    let name = non_empty(name, "host function")?;
    Ok(WasmHostReturn {
        module: module.to_string(),
        name: name.to_string(),
        value: parse_scalar_arg(scalar)?,
    })
}

/// Rejects non-finite floating-point arguments at the CLI boundary.
fn require_finite(value: impl Into<f64>, source: &str) -> Result<(), String> {
    if value.into().is_finite() {
        Ok(())
    } else {
        Err(invalid_scalar(source))
    }
}

/// Renders the stable invalid-scalar diagnostic family.
fn invalid_scalar(value: &str) -> String {
    format!("wasm-argument-invalid: invalid scalar argument `{value}`")
}

/// Validates and executes one compiler-emitted WASM artifact configuration.
fn execute(config: &WasmRunConfig) -> Result<String, String> {
    let bytes = fs::read(&config.artifact).map_err(|err| {
        format!(
            "wasm-artifact-unreadable: cannot read `{}`: {err}",
            config.artifact.display()
        )
    })?;
    let manifest = load_manifest(&config.artifact)?;
    validate_manifest(&manifest, &bytes)?;
    let export = select_export(&manifest, config.export.as_deref())?;
    validate_arguments(export, &config.args)?;
    validate_module(&bytes).map_err(|err| {
        runtime_diagnostic(
            "wasm-runtime-trap",
            &manifest,
            export,
            &config.artifact,
            &err.to_string(),
        )
    })?;

    let encoded_args = serde_json::to_string(&config.args)
        .map_err(|err| format!("wasm-argument-invalid: cannot encode arguments: {err}"))?;
    let encoded_hosts = serde_json::to_string(&config.host_returns)
        .map_err(|err| format!("wasm-argument-invalid: cannot encode host imports: {err}"))?;
    let runtime = std::env::var("TERLAN_WASM_RUNTIME").unwrap_or_else(|_| "node".to_string());
    let mut command = Command::new(&runtime);
    command
        .arg("--eval")
        .arg(NODE_WASM_RUNNER)
        .arg(&config.artifact)
        .arg(&export.name)
        .arg(encoded_args)
        .arg(config.repeat.to_string())
        .arg(encoded_hosts);
    let output = run_command_with_timeout(&mut command, "Wasm runtime", config.timeout).map_err(
        |message| {
            let classified = classify_runner_error(message);
            contextualize_runtime_diagnostic(&classified, &manifest, export, &config.artifact)
        },
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let classified = classify_runtime_stderr(&stderr);
        return Err(contextualize_runtime_diagnostic(
            &classified,
            &manifest,
            export,
            &config.artifact,
        ));
    }
    let result = String::from_utf8(output.stdout)
        .map_err(|_| "wasm-runtime-trap: runtime result was not UTF-8".to_string())?;
    if result.is_empty() {
        return Err("wasm-runtime-trap: runtime returned an empty result".to_string());
    }
    validate_result(export, &result).map_err(|message| {
        contextualize_runtime_diagnostic(&message, &manifest, export, &config.artifact)
    })?;
    validate_expected_result(export, config.expected.as_ref(), &result).map_err(|message| {
        contextualize_runtime_diagnostic(&message, &manifest, export, &config.artifact)
    })?;
    Ok(result)
}

/// Executes one zero-arity boolean Wasm test export.
pub(crate) fn execute_test_export(artifact: &Path, export: &str) -> Result<(), String> {
    let config = WasmRunConfig {
        artifact: artifact.to_path_buf(),
        export: Some(export.to_string()),
        args: Vec::new(),
        host_returns: Vec::new(),
        expected: Some(WasmScalarArg {
            kind: "i32".to_string(),
            value: "1".to_string(),
        }),
        repeat: 1,
        timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
    };
    execute(&config).map(|_| ())
}

/// Returns the sidecar manifest path for a WASM artifact.
fn manifest_path(artifact: &Path) -> PathBuf {
    PathBuf::from(format!("{}.json", artifact.display()))
}

/// Reads and decodes the sidecar manifest for a WASM artifact.
fn load_manifest(artifact: &Path) -> Result<WasmArtifactManifest, String> {
    let path = manifest_path(artifact);
    let source = fs::read_to_string(&path).map_err(|err| {
        format!(
            "wasm-manifest-missing: cannot read `{}`: {err}",
            path.display()
        )
    })?;
    serde_json::from_str(&source)
        .map_err(|err| format!("wasm-manifest-invalid: `{}`: {err}", path.display()))
}

/// Verifies artifact kind, byte checksum, and ABI signature checksums.
fn validate_manifest(manifest: &WasmArtifactManifest, bytes: &[u8]) -> Result<(), String> {
    if manifest.schema_version != MANIFEST_SCHEMA || manifest.artifact_kind != ARTIFACT_KIND {
        return Err("wasm-manifest-invalid: unsupported artifact manifest contract".to_string());
    }
    let actual = wasm_checksum(bytes);
    if manifest.checksum != actual {
        return Err(format!(
            "wasm-artifact-stale: manifest checksum `{}` does not match `{actual}`",
            manifest.checksum
        ));
    }
    let current_contract = wasm_abi_contract_checksum();
    if manifest.abi_contract_checksum != current_contract {
        return Err(format!(
            "wasm-abi-contract-stale: manifest namespace checksum `{}` does not match `{current_contract}`",
            manifest.abi_contract_checksum
        ));
    }
    let signatures = manifest
        .exports
        .iter()
        .map(|export| WasmAbiSignature {
            name: export.name.clone(),
            params: export.params.iter().map(|param| param.ty.clone()).collect(),
            result: export.result.clone(),
        })
        .collect::<Vec<_>>();
    let current_signature = wasm_abi_signature_checksum(&signatures);
    if manifest.signature_checksum != current_signature {
        return Err(format!(
            "wasm-abi-signature-stale: manifest signature checksum `{}` does not match `{current_signature}`",
            manifest.signature_checksum
        ));
    }
    Ok(())
}

/// Selects an explicit export or the artifact's sole exported function.
fn select_export<'a>(
    manifest: &'a WasmArtifactManifest,
    requested: Option<&str>,
) -> Result<&'a WasmExportManifest, String> {
    if let Some(name) = requested {
        return manifest
            .exports
            .iter()
            .find(|export| export.name == name)
            .ok_or_else(|| {
                format!("wasm-export-missing: `{name}` is not in the artifact manifest")
            });
    }
    match manifest.exports.as_slice() {
        [export] => Ok(export),
        [] => Err("wasm-export-missing: artifact manifest has no exports".to_string()),
        _ => Err("wasm-export-missing: --export is required for multiple exports".to_string()),
    }
}

/// Validates argument count and scalar types against an export signature.
fn validate_arguments(export: &WasmExportManifest, args: &[WasmScalarArg]) -> Result<(), String> {
    if export.params.len() != args.len() {
        return Err(format!(
            "wasm-argument-invalid: export `{}` expects {} arguments, got {}",
            export.name,
            export.params.len(),
            args.len()
        ));
    }
    for (index, (param, arg)) in export.params.iter().zip(args).enumerate() {
        if param.ty != arg.kind || !is_scalar_type(&param.ty) {
            return Err(format!(
                "wasm-argument-invalid: argument {index} for `{}` expects `{}`, got `{}`",
                export.name, param.ty, arg.kind
            ));
        }
    }
    if !is_scalar_type(&export.result) {
        return Err(format!(
            "wasm-argument-invalid: export `{}` has unsupported result `{}`",
            export.name, export.result
        ));
    }
    Ok(())
}

/// Reports whether a manifest type belongs to the supported scalar ABI.
fn is_scalar_type(value: &str) -> bool {
    matches!(value, "i32" | "i64" | "f32" | "f64")
}

/// Validates runner output against the export result scalar type.
fn validate_result(export: &WasmExportManifest, result: &str) -> Result<(), String> {
    let valid = match export.result.as_str() {
        "i32" => result.parse::<i32>().is_ok(),
        "i64" => result.parse::<i64>().is_ok(),
        "f32" => result.parse::<f32>().is_ok_and(f32::is_finite),
        "f64" => result.parse::<f64>().is_ok_and(f64::is_finite),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "wasm-runtime-trap: export `{}` returned invalid `{}` value `{result}`",
            export.name, export.result
        ))
    }
}

/// Compares optional expected output using the export's scalar semantics.
fn validate_expected_result(
    export: &WasmExportManifest,
    expected: Option<&WasmScalarArg>,
    result: &str,
) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected.kind != export.result {
        return Err(format!(
            "wasm-result-mismatch: expected type `{}` does not match export result `{}`",
            expected.kind, export.result
        ));
    }
    let matches = match expected.kind.as_str() {
        "i32" => expected.value.parse::<i32>().ok() == result.parse::<i32>().ok(),
        "i64" => expected.value.parse::<i64>().ok() == result.parse::<i64>().ok(),
        "f32" => expected.value.parse::<f32>().ok() == result.parse::<f32>().ok(),
        "f64" => expected.value.parse::<f64>().ok() == result.parse::<f64>().ok(),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(format!(
            "wasm-result-mismatch: expected {}:{}, received {result}",
            expected.kind, expected.value
        ))
    }
}

/// Maps process-runner failures into stable WASM diagnostic families.
fn classify_runner_error(message: String) -> String {
    if message.contains("timed out") {
        format!("wasm-exec-timeout: {message}")
    } else {
        format!("wasm-runtime-unavailable: {message}")
    }
}

/// Preserves known runner diagnostics and classifies unknown stderr as a trap.
fn classify_runtime_stderr(stderr: &str) -> String {
    for family in [
        "wasm-import-missing",
        "wasm-import-unsupported",
        "wasm-export-missing",
        "wasm-export-unsupported",
        "wasm-runtime-trap",
    ] {
        if stderr.starts_with(family) {
            return stderr.to_string();
        }
    }
    format!("wasm-runtime-trap: {stderr}")
}

/// Adds module, export, and artifact context to a runner diagnostic.
fn contextualize_runtime_diagnostic(
    diagnostic: &str,
    manifest: &WasmArtifactManifest,
    export: &WasmExportManifest,
    artifact: &Path,
) -> String {
    let (family, detail) = diagnostic
        .split_once(':')
        .unwrap_or(("wasm-runtime-trap", diagnostic));
    runtime_diagnostic(family, manifest, export, artifact, detail.trim())
}

/// Renders one stable contextual WASM runtime diagnostic.
fn runtime_diagnostic(
    family: &str,
    manifest: &WasmArtifactManifest,
    export: &WasmExportManifest,
    artifact: &Path,
    detail: &str,
) -> String {
    format!(
        "{family}: {}.{} [{}]: {detail}",
        manifest.module,
        export.name,
        artifact.display()
    )
}

#[cfg(test)]
#[path = "wasm_runtime_test.rs"]
#[cfg(test)]
mod wasm_runtime_test;
