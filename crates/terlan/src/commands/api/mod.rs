use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use openapiv3::{OpenAPI, ReferenceOr};
use serde::{Deserialize, Serialize};

use crate::compiler::api_contract::{ApiContract, API_CONTRACT_SCHEMA, OPENAPI_VERSION};
use crate::{CliCommand, CliState};

const API_OUTPUT_DIR: &str = "api";
const API_CONTRACT_FILE: &str = "api-contract.json";
const OPENAPI_JSON_FILE: &str = "openapi.json";
const OPENAPI_YAML_FILE: &str = "openapi.yaml";
const API_IMPORT_SKIP_FILE: &str = "api-import-skips.json";
const DEFAULT_SERVICE_NAME: &str = "Terlan API";
const DEFAULT_SERVICE_VERSION: &str = "0.0.0";

/// Executes the `terlc api` command group.
///
/// Inputs:
/// - `cmd`: parsed CLI command whose first argument is the API subcommand.
/// - `state`: global CLI state including output directory and incremental
///   write behavior.
///
/// Output:
/// - `ExitCode::SUCCESS` when the selected API operation succeeds.
/// - `ExitCode::from(2)` for malformed command-local arguments.
/// - `ExitCode::from(1)` for filesystem, serialization, or validation errors.
///
/// Transformation:
/// - Dispatches API contract emission, artifact validation, and future OpenAPI
///   ingestion through one public command surface.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    match parse_api_command(&cmd.args) {
        ApiCommand::Help => {
            print_api_usage();
            ExitCode::SUCCESS
        }
        ApiCommand::Emit(args) => match emit_api_artifacts(&state.out_dir, &args) {
            Ok(paths) => {
                println!("wrote {}", paths.contract.display());
                println!("wrote {}", paths.openapi_json.display());
                println!("wrote {}", paths.openapi_yaml.display());
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        ApiCommand::Check(args) => match check_api_artifacts(&state.out_dir, &args) {
            Ok(()) => {
                println!("api artifacts are valid");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        ApiCommand::Import(args) => match import_openapi_client(&args) {
            Ok(paths) => {
                println!("wrote {}", paths.module.display());
                println!("wrote {}", paths.skip_manifest.display());
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        ApiCommand::Error(message) => {
            eprintln!("{message}");
            print_api_usage();
            ExitCode::from(2)
        }
    }
}

/// Parsed API command group variant.
///
/// Inputs:
/// - Produced from command-local arguments after `terlc api`.
///
/// Output:
/// - Help, emit, check, import, or usage error.
///
/// Transformation:
/// - Gives each subcommand a typed argument payload so command execution stays
///   separate from argument parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ApiCommand {
    Help,
    Emit(ApiEmitArgs),
    Check(ApiCheckArgs),
    Import(ApiImportArgs),
    Error(String),
}

/// Arguments for `terlc api emit`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiEmitArgs {
    service_name: String,
    service_version: String,
    source: Option<PathBuf>,
}

/// Arguments for `terlc api check`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiCheckArgs {
    api_dir: Option<PathBuf>,
}

/// Arguments for `terlc api import`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiImportArgs {
    input: PathBuf,
    module: String,
    out_dir: PathBuf,
}

/// Paths written by `terlc api import`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiImportPaths {
    module: PathBuf,
    skip_manifest: PathBuf,
}

/// Paths written by `terlc api emit`.
///
/// Inputs:
/// - Computed from the global compiler output directory.
///
/// Output:
/// - Concrete artifact paths for command output and tests.
///
/// Transformation:
/// - Keeps path construction centralized so emit and check use the same layout.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiArtifactPaths {
    contract: PathBuf,
    openapi_json: PathBuf,
    openapi_yaml: PathBuf,
}

/// Parses `terlc api` command-local arguments.
///
/// Inputs:
/// - `args`: arguments after the `api` verb.
///
/// Output:
/// - Typed API command or usage error.
///
/// Transformation:
/// - Routes the first positional argument as a subcommand and delegates
///   subcommand-specific flags to focused parsers.
fn parse_api_command(args: &[String]) -> ApiCommand {
    match args {
        [] => ApiCommand::Error("terlc api requires a subcommand: emit, check, import".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => ApiCommand::Help,
        [subcommand, rest @ ..] if subcommand == "emit" => parse_api_emit_args(rest),
        [subcommand, rest @ ..] if subcommand == "check" => parse_api_check_args(rest),
        [subcommand, rest @ ..] if subcommand == "import" => parse_api_import_args(rest),
        [subcommand, ..] => {
            ApiCommand::Error(format!("unknown terlc api subcommand: {subcommand}"))
        }
    }
}

/// Parses `api emit` arguments.
///
/// Inputs:
/// - `args`: command-local arguments after `api emit`.
///
/// Output:
/// - Emit arguments or usage error.
///
/// Transformation:
/// - Accepts optional service metadata and a source router file while output
///   location remains the global `--out-dir` selected by the top-level CLI
///   parser.
fn parse_api_emit_args(args: &[String]) -> ApiCommand {
    let mut service_name = DEFAULT_SERVICE_NAME.to_string();
    let mut service_version = DEFAULT_SERVICE_VERSION.to_string();
    let mut source = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return ApiCommand::Help,
            "--service-name" => {
                let Some(value) = args.get(index + 1) else {
                    return ApiCommand::Error(
                        "terlc api emit --service-name requires a value".to_string(),
                    );
                };
                service_name = value.clone();
                index += 2;
            }
            "--service-version" => {
                let Some(value) = args.get(index + 1) else {
                    return ApiCommand::Error(
                        "terlc api emit --service-version requires a value".to_string(),
                    );
                };
                service_version = value.clone();
                index += 2;
            }
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    return ApiCommand::Error(
                        "terlc api emit --source requires a value".to_string(),
                    );
                };
                source = Some(PathBuf::from(value));
                index += 2;
            }
            other => {
                return ApiCommand::Error(format!("unexpected terlc api emit argument: {other}"))
            }
        }
    }

    ApiCommand::Emit(ApiEmitArgs {
        service_name,
        service_version,
        source,
    })
}

/// Parses `api check` arguments.
///
/// Inputs:
/// - `args`: command-local arguments after `api check`.
///
/// Output:
/// - Check arguments or usage error.
///
/// Transformation:
/// - Accepts an optional artifact directory override. When omitted, check uses
///   the same global output directory layout as emit.
fn parse_api_check_args(args: &[String]) -> ApiCommand {
    match args {
        [] => ApiCommand::Check(ApiCheckArgs { api_dir: None }),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => ApiCommand::Help,
        [flag, value] if flag == "--api-dir" => ApiCommand::Check(ApiCheckArgs {
            api_dir: Some(PathBuf::from(value)),
        }),
        [arg, ..] => ApiCommand::Error(format!("unexpected terlc api check argument: {arg}")),
    }
}

/// Parses `api import` arguments.
///
/// Inputs:
/// - `args`: command-local arguments after `api import`.
///
/// Output:
/// - Import arguments or usage error.
///
/// Transformation:
/// - Requires one input OpenAPI document plus explicit generated module and
///   output directory, even though execution is deferred to a later slice.
fn parse_api_import_args(args: &[String]) -> ApiCommand {
    if matches!(args, [flag] if matches!(flag.as_str(), "--help" | "-h")) {
        return ApiCommand::Help;
    }
    if args.is_empty() {
        return ApiCommand::Error("terlc api import requires an OpenAPI input path".to_string());
    }

    let mut input = None;
    let mut module = None;
    let mut out_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--module" => {
                let Some(value) = args.get(index + 1) else {
                    return ApiCommand::Error(
                        "terlc api import --module requires a value".to_string(),
                    );
                };
                module = Some(value.clone());
                index += 2;
            }
            "--out" => {
                let Some(value) = args.get(index + 1) else {
                    return ApiCommand::Error(
                        "terlc api import --out requires a value".to_string(),
                    );
                };
                out_dir = Some(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with('-') => {
                return ApiCommand::Error(format!("unexpected terlc api import option: {value}"));
            }
            value => {
                if input.is_some() {
                    return ApiCommand::Error(
                        "terlc api import accepts exactly one OpenAPI input path".to_string(),
                    );
                }
                input = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }

    let Some(input) = input else {
        return ApiCommand::Error("terlc api import requires an OpenAPI input path".to_string());
    };
    let Some(module) = module else {
        return ApiCommand::Error("terlc api import requires --module <Module.Name>".to_string());
    };
    let Some(out_dir) = out_dir else {
        return ApiCommand::Error("terlc api import requires --out <dir>".to_string());
    };

    ApiCommand::Import(ApiImportArgs {
        input,
        module,
        out_dir,
    })
}

/// Writes deterministic API contract and OpenAPI artifacts.
///
/// Inputs:
/// - `out_dir`: global compiler output directory.
/// - `args`: parsed API emit arguments.
///
/// Output:
/// - Artifact paths when all writes succeed.
///
/// Transformation:
/// - Builds the compiler-owned API contract from the optional router source,
///   projects it to OpenAPI, and writes JSON/YAML artifacts under
///   `<out-dir>/api`.
fn emit_api_artifacts(out_dir: &Path, args: &ApiEmitArgs) -> Result<ApiArtifactPaths, String> {
    let paths = api_artifact_paths(out_dir);
    let api_dir = paths
        .contract
        .parent()
        .ok_or_else(|| "internal error: API contract path has no parent".to_string())?;
    fs::create_dir_all(api_dir).map_err(|err| {
        format!(
            "cannot create API artifact directory {}: {err}",
            api_dir.display()
        )
    })?;

    let contract = api_contract_from_emit_args(args)?;
    let openapi = contract.to_openapi();
    let contract_json = serde_json::to_string_pretty(&contract)
        .map_err(|err| format!("cannot serialize API contract: {err}"))?;
    let openapi_json = serde_json::to_string_pretty(&openapi)
        .map_err(|err| format!("cannot serialize OpenAPI JSON: {err}"))?;
    let openapi_yaml = serde_yaml::to_string(&openapi)
        .map_err(|err| format!("cannot serialize OpenAPI YAML: {err}"))?;

    write_text_artifact(&paths.contract, &contract_json)?;
    write_text_artifact(&paths.openapi_json, &openapi_json)?;
    write_text_artifact(&paths.openapi_yaml, &openapi_yaml)?;
    Ok(paths)
}

/// Builds an API contract from parsed emit arguments.
///
/// Inputs:
/// - `args`: parsed `api emit` arguments.
///
/// Output:
/// - Empty contract when no source file is supplied.
/// - Route-bearing contract when `--source` points at a Terlan router module.
///
/// Transformation:
/// - Reads the source file at the command boundary and delegates route
///   extraction to the compiler-owned API contract model.
fn api_contract_from_emit_args(args: &ApiEmitArgs) -> Result<ApiContract, String> {
    if let Some(source) = &args.source {
        let text = fs::read_to_string(source).map_err(|err| {
            format!(
                "error[api_emit]: cannot read API source {}: {err}",
                source.display()
            )
        })?;
        ApiContract::from_router_source(&text, &args.service_name, &args.service_version)
    } else {
        Ok(ApiContract::empty(
            &args.service_name,
            &args.service_version,
        ))
    }
}

/// Imports an OpenAPI document into an initial Terlan client module.
///
/// Inputs:
/// - `args`: parsed import arguments containing source document, module name,
///   and output directory.
///
/// Output:
/// - Paths to the generated module and skip manifest.
///
/// Transformation:
/// - Parses JSON or YAML through `openapiv3`, emits deterministic method/path
///   helpers for supported operations, and records unsupported references in a
///   manifest rather than silently dropping them.
fn import_openapi_client(args: &ApiImportArgs) -> Result<ApiImportPaths, String> {
    let text = fs::read_to_string(&args.input).map_err(|err| {
        format!(
            "error[api_import]: cannot read OpenAPI document {}: {err}",
            args.input.display()
        )
    })?;
    let openapi: OpenAPI = if args
        .input
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        serde_json::from_str(&text).map_err(|err| {
            format!(
                "error[api_import]: cannot parse OpenAPI JSON {}: {err}",
                args.input.display()
            )
        })?
    } else {
        serde_yaml::from_str(&text).map_err(|err| {
            format!(
                "error[api_import]: cannot parse OpenAPI YAML {}: {err}",
                args.input.display()
            )
        })?
    };

    fs::create_dir_all(&args.out_dir).map_err(|err| {
        format!(
            "error[api_import]: cannot create output directory {}: {err}",
            args.out_dir.display()
        )
    })?;
    let operations = imported_operations(&openapi);
    let skips = import_skips(&openapi);
    let module_path = args
        .out_dir
        .join(format!("{}.terl", module_file_stem(&args.module)));
    let skip_manifest = args.out_dir.join(API_IMPORT_SKIP_FILE);
    write_text_artifact(
        &module_path,
        &render_imported_client_module(&args.module, &operations),
    )?;
    let skip_json = serde_json::to_string_pretty(&ImportSkipManifest {
        schema: "terlan-api-import-skips-v1",
        source: args.input.display().to_string(),
        module: args.module.clone(),
        skips,
    })
    .map_err(|err| format!("error[api_import]: cannot serialize skip manifest: {err}"))?;
    write_text_artifact(&skip_manifest, &skip_json)?;
    Ok(ApiImportPaths {
        module: module_path,
        skip_manifest,
    })
}

/// Supported imported OpenAPI operation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportedOperation {
    method: String,
    path: String,
    function: String,
    summary: Option<String>,
    description: Option<String>,
}

/// One skipped OpenAPI feature recorded during import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ImportSkip {
    path: String,
    reason: String,
}

/// Skip manifest for API import.
#[derive(Debug, Serialize)]
struct ImportSkipManifest {
    schema: &'static str,
    source: String,
    module: String,
    skips: Vec<ImportSkip>,
}

/// Extracts supported operations from an OpenAPI document.
fn imported_operations(openapi: &OpenAPI) -> Vec<ImportedOperation> {
    let mut operations = openapi
        .operations()
        .filter(|(_, method, _)| supported_http_method(method))
        .map(|(path, method, operation)| ImportedOperation {
            method: method.to_uppercase(),
            path: path.to_string(),
            function: operation
                .operation_id
                .as_deref()
                .map(snake_case_identifier)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| generated_operation_name(method, path)),
            summary: operation.summary.clone(),
            description: operation.description.clone(),
        })
        .collect::<Vec<_>>();
    operations.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.method.cmp(&right.method))
            .then(left.function.cmp(&right.function))
    });
    operations
}

/// Records unsupported OpenAPI features for deterministic diagnostics.
fn import_skips(openapi: &OpenAPI) -> Vec<ImportSkip> {
    let mut skips = Vec::new();
    for (path, item) in openapi.paths.iter() {
        if matches!(item, ReferenceOr::Reference { .. }) {
            skips.push(ImportSkip {
                path: path.clone(),
                reason: "path references are not imported in this slice".to_string(),
            });
        }
        let ReferenceOr::Item(item) = item else {
            continue;
        };
        if item.trace.is_some() {
            skips.push(ImportSkip {
                path: path.clone(),
                reason: "TRACE operations are not imported in this slice".to_string(),
            });
        }
    }
    skips.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.reason.cmp(&right.reason))
    });
    skips
}

/// Renders an imported Terlan API client module.
fn render_imported_client_module(module: &str, operations: &[ImportedOperation]) -> String {
    let mut output = String::new();
    output.push_str("module ");
    output.push_str(module);
    output.push_str(".\n\n");
    output.push_str("/**\n");
    output.push_str(" * Generated OpenAPI client metadata.\n");
    output.push_str(" *\n");
    output.push_str(" * This module is generated by `terlc api import`.\n");
    output.push_str(" */\n\n");
    for operation in operations {
        push_operation_docs(&mut output, operation);
        output.push_str("pub ");
        output.push_str(&operation.function);
        output.push_str("_method(): String ->\n    ");
        output.push_str(&terlan_string_literal(&operation.method));
        output.push_str(".\n\n");
        push_operation_docs(&mut output, operation);
        output.push_str("pub ");
        output.push_str(&operation.function);
        output.push_str("_path(): String ->\n    ");
        output.push_str(&terlan_string_literal(&operation.path));
        output.push_str(".\n\n");
    }
    output
}

/// Appends operation documentation to generated source.
fn push_operation_docs(output: &mut String, operation: &ImportedOperation) {
    output.push_str("/**\n");
    if let Some(summary) = operation
        .summary
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        output.push_str(" * ");
        output.push_str(&doc_comment_text(summary));
        output.push('\n');
    } else {
        output.push_str(" * Imported OpenAPI operation.\n");
    }
    if let Some(description) = operation
        .description
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        output.push_str(" *\n");
        for line in description.lines() {
            output.push_str(" * ");
            output.push_str(&doc_comment_text(line));
            output.push('\n');
        }
    }
    output.push_str(" *\n");
    output.push_str(" * Input: no arguments.\n");
    output.push_str(" * Output: generated operation metadata.\n");
    output
        .push_str(" * Transformation: preserves OpenAPI method/path metadata in Terlan source.\n");
    output.push_str(" */\n");
}

/// Escapes text for generated block doc comments.
fn doc_comment_text(value: &str) -> String {
    value.replace("*/", "* /")
}

/// Renders a Terlan string literal.
fn terlan_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Returns whether an OpenAPI method is imported in this slice.
fn supported_http_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch"
    )
}

/// Builds a generated operation name from method and path.
fn generated_operation_name(method: &str, path: &str) -> String {
    let mut name = String::new();
    name.push_str(method);
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        name.push('_');
        name.push_str(segment.trim_matches(|ch| ch == '{' || ch == '}'));
    }
    snake_case_identifier(&name)
}

/// Converts text into a Terlan lower-case identifier.
fn snake_case_identifier(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && !previous_was_separator && !output.ends_with('_') {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !output.is_empty() && !output.ends_with('_') {
            output.push('_');
            previous_was_separator = true;
        }
    }
    output.trim_matches('_').to_string()
}

/// Selects the module filename for generated client output.
fn module_file_stem(module: &str) -> String {
    module
        .rsplit('.')
        .next()
        .map(snake_case_identifier)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "client".to_string())
}

/// Validates emitted API artifacts.
///
/// Inputs:
/// - `out_dir`: global compiler output directory.
/// - `args`: parsed API check arguments.
///
/// Output:
/// - `Ok(())` when all required artifacts parse and match stable schema fields.
///
/// Transformation:
/// - Reads the deterministic artifact set, parses JSON/YAML through maintained
///   serde libraries, and checks the compiler-owned schema marker plus OpenAPI
///   version.
fn check_api_artifacts(out_dir: &Path, args: &ApiCheckArgs) -> Result<(), String> {
    let api_dir = args
        .api_dir
        .clone()
        .unwrap_or_else(|| out_dir.join(API_OUTPUT_DIR));
    let contract: ContractCheck = read_json(&api_dir.join(API_CONTRACT_FILE))?;
    if contract.schema != API_CONTRACT_SCHEMA {
        return Err(format!(
            "error[api_check]: API contract schema `{}` does not match `{}`",
            contract.schema, API_CONTRACT_SCHEMA
        ));
    }

    let openapi_json: OpenApiCheck = read_json(&api_dir.join(OPENAPI_JSON_FILE))?;
    validate_openapi_check("openapi.json", &openapi_json)?;
    let openapi_yaml: OpenApiCheck = read_yaml(&api_dir.join(OPENAPI_YAML_FILE))?;
    validate_openapi_check("openapi.yaml", &openapi_yaml)?;
    Ok(())
}

/// Builds API artifact paths for one output directory.
fn api_artifact_paths(out_dir: &Path) -> ApiArtifactPaths {
    let api_dir = out_dir.join(API_OUTPUT_DIR);
    ApiArtifactPaths {
        contract: api_dir.join(API_CONTRACT_FILE),
        openapi_json: api_dir.join(OPENAPI_JSON_FILE),
        openapi_yaml: api_dir.join(OPENAPI_YAML_FILE),
    }
}

/// Writes one text artifact with a trailing newline.
fn write_text_artifact(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, format!("{text}\n"))
        .map_err(|err| format!("cannot write API artifact {}: {err}", path.display()))
}

/// Reads one JSON artifact into a typed validation shape.
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("error[api_check]: cannot read {}: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("error[api_check]: cannot parse {}: {err}", path.display()))
}

/// Reads one YAML artifact into a typed validation shape.
fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("error[api_check]: cannot read {}: {err}", path.display()))?;
    serde_yaml::from_str(&text)
        .map_err(|err| format!("error[api_check]: cannot parse {}: {err}", path.display()))
}

/// Validates minimal OpenAPI artifact identity.
fn validate_openapi_check(label: &str, openapi: &OpenApiCheck) -> Result<(), String> {
    if openapi.openapi != OPENAPI_VERSION {
        return Err(format!(
            "error[api_check]: {label} version `{}` does not match `{}`",
            openapi.openapi, OPENAPI_VERSION
        ));
    }
    Ok(())
}

/// Minimal API contract validation shape.
#[derive(Debug, Deserialize)]
struct ContractCheck {
    schema: String,
}

/// Minimal OpenAPI validation shape.
#[derive(Debug, Deserialize)]
struct OpenApiCheck {
    openapi: String,
}

/// Prints API command group usage.
fn print_api_usage() {
    println!(
        "terlc api emit [--source <file.terl>] [--service-name <name>] [--service-version <version>] [--out-dir <dir>]"
    );
    println!("terlc api check [--api-dir <dir>]");
    println!("terlc api import <openapi.yaml|openapi.json> --module <Module.Name> --out <dir>");
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
