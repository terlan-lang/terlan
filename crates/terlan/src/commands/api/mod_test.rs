use std::fs;
use std::process::ExitCode;

use super::*;
use crate::support::test_fs::temp_dir;

/// Parses default API emission arguments.
///
/// Inputs:
/// - `api emit` with no command-local flags.
///
/// Output:
/// - Test passes when default service metadata is selected.
///
/// Transformation:
/// - Exercises command parsing without writing artifacts.
#[test]
fn parse_api_emit_uses_default_service_metadata() {
    assert_eq!(
        parse_api_command(&["emit".to_string()]),
        ApiCommand::Emit(ApiEmitArgs {
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            service_version: DEFAULT_SERVICE_VERSION.to_string(),
            source: None,
        })
    );
}

/// Parses explicit API emission service metadata.
///
/// Inputs:
/// - `api emit --service-name ... --service-version ...`.
///
/// Output:
/// - Test passes when the parser preserves both values.
///
/// Transformation:
/// - Keeps API service identity configurable without requiring a project
///   manifest in this first slice.
#[test]
fn parse_api_emit_accepts_service_metadata() {
    assert_eq!(
        parse_api_command(&[
            "emit".to_string(),
            "--service-name".to_string(),
            "Billing".to_string(),
            "--service-version".to_string(),
            "1.2.3".to_string(),
        ]),
        ApiCommand::Emit(ApiEmitArgs {
            service_name: "Billing".to_string(),
            service_version: "1.2.3".to_string(),
            source: None,
        })
    );
}

/// Parses API emission with a source router module.
///
/// Inputs:
/// - `api emit --source src/app/Http.terl`.
///
/// Output:
/// - Test passes when the parser preserves the source path.
///
/// Transformation:
/// - Locks the source-driven API emission surface used by route extraction.
#[test]
fn parse_api_emit_accepts_source_router_file() {
    assert_eq!(
        parse_api_command(&[
            "emit".to_string(),
            "--source".to_string(),
            "src/app/Http.terl".to_string(),
        ]),
        ApiCommand::Emit(ApiEmitArgs {
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            service_version: DEFAULT_SERVICE_VERSION.to_string(),
            source: Some("src/app/Http.terl".into()),
        })
    );
}

/// Emits deterministic API contract and OpenAPI artifacts.
///
/// Inputs:
/// - Temporary output directory and explicit service metadata.
///
/// Output:
/// - Test passes when all initial API artifacts are written with stable
///   identity fields.
///
/// Transformation:
/// - Runs through the public command entry point rather than calling the model
///   directly.
#[test]
fn api_emit_writes_contract_json_and_openapi_outputs() {
    let out_dir = temp_dir("api_command", "emit_outputs");
    let mut state = CliState::default();
    state.out_dir = out_dir.clone();
    let code = run(
        CliCommand {
            verb: Some("api".to_string()),
            args: vec![
                "emit".to_string(),
                "--service-name".to_string(),
                "Billing".to_string(),
                "--service-version".to_string(),
                "1.2.3".to_string(),
            ],
        },
        state,
    );

    assert_eq!(code, ExitCode::SUCCESS);
    let contract = fs::read_to_string(out_dir.join("api/api-contract.json"))
        .expect("read API contract artifact");
    assert!(contract.contains("\"schema\": \"terlan-api-contract-v1\""));
    assert!(contract.contains("\"name\": \"Billing\""));

    let openapi_json =
        fs::read_to_string(out_dir.join("api/openapi.json")).expect("read OpenAPI JSON artifact");
    assert!(openapi_json.contains("\"openapi\": \"3.1.0\""));
    assert!(openapi_json.contains("\"title\": \"Billing\""));

    let openapi_yaml =
        fs::read_to_string(out_dir.join("api/openapi.yaml")).expect("read OpenAPI YAML artifact");
    assert!(openapi_yaml.contains("openapi: 3.1.0"));
    assert!(openapi_yaml.contains("title: Billing"));
}

/// Emits route-bearing API contract and OpenAPI artifacts.
///
/// Inputs:
/// - Temporary output directory and one source router module.
///
/// Output:
/// - Test passes when the generated contract and OpenAPI file contain the
///   discovered route.
///
/// Transformation:
/// - Exercises the command boundary, source-file read, router extraction, and
///   OpenAPI projection together.
#[test]
fn api_emit_from_source_writes_route_openapi_paths() {
    let out_dir = temp_dir("api_command", "emit_source_outputs");
    let source_path = out_dir.join("Http.terl");
    write_router_source(&source_path);

    let mut state = CliState::default();
    state.out_dir = out_dir.clone();
    let code = run(
        CliCommand {
            verb: Some("api".to_string()),
            args: vec![
                "emit".to_string(),
                "--source".to_string(),
                source_path.display().to_string(),
                "--service-name".to_string(),
                "Billing".to_string(),
            ],
        },
        state,
    );

    assert_eq!(code, ExitCode::SUCCESS);
    let contract = fs::read_to_string(out_dir.join("api/api-contract.json"))
        .expect("read API contract artifact");
    assert!(contract.contains("\"path\": \"/users/:id\""));
    assert!(contract.contains("\"handler\": \"show_user\""));

    let openapi_json =
        fs::read_to_string(out_dir.join("api/openapi.json")).expect("read OpenAPI JSON artifact");
    assert!(openapi_json.contains("\"/users/{id}\""));
    assert!(openapi_json.contains("\"get\""));
    assert!(openapi_json.contains("\"operationId\": \"get_show_user\""));
}

/// Validates previously emitted API artifacts.
///
/// Inputs:
/// - Temporary output directory populated by `api emit`.
///
/// Output:
/// - Test passes when `api check` accepts the emitted artifact set.
///
/// Transformation:
/// - Confirms emission and validation share the same deterministic layout.
#[test]
fn api_check_accepts_emitted_outputs() {
    let out_dir = temp_dir("api_command", "check_outputs");
    let mut emit_state = CliState::default();
    emit_state.out_dir = out_dir.clone();
    assert_eq!(
        run(
            CliCommand {
                verb: Some("api".to_string()),
                args: vec!["emit".to_string()],
            },
            emit_state,
        ),
        ExitCode::SUCCESS
    );

    let mut check_state = CliState::default();
    check_state.out_dir = out_dir;
    assert_eq!(
        run(
            CliCommand {
                verb: Some("api".to_string()),
                args: vec!["check".to_string()],
            },
            check_state,
        ),
        ExitCode::SUCCESS
    );
}

/// Parses import arguments before implementation is available.
///
/// Inputs:
/// - `api import` with input path, module, and output directory.
///
/// Output:
/// - Test passes when the parser captures the future import request shape.
///
/// Transformation:
/// - Locks the public command shape while implementation remains a later
///   roadmap slice.
#[test]
fn parse_api_import_requires_module_and_output_directory() {
    assert_eq!(
        parse_api_command(&[
            "import".to_string(),
            "openapi.yaml".to_string(),
            "--module".to_string(),
            "external.GitHub".to_string(),
            "--out".to_string(),
            "src/external/GitHub".to_string(),
        ]),
        ApiCommand::Import(ApiImportArgs {
            input: "openapi.yaml".into(),
            module: "external.GitHub".to_string(),
            out_dir: "src/external/GitHub".into(),
        })
    );
}

/// Imports a minimal OpenAPI document into Terlan source.
///
/// Inputs:
/// - One OpenAPI YAML fixture with a GET operation.
///
/// Output:
/// - Test passes when the generated module exposes method/path helpers and a
///   skip manifest.
///
/// Transformation:
/// - Exercises maintained OpenAPI parsing, deterministic generation, and output
///   layout through the public API import command.
#[test]
fn api_import_generates_client_module_and_skip_manifest() {
    let out_dir = temp_dir("api_command", "import_outputs");
    let input = out_dir.join("openapi.yaml");
    fs::write(
        &input,
        "openapi: 3.0.3\ninfo:\n  title: Billing\n  version: 1.2.3\npaths:\n  /users/{id}:\n    get:\n      operationId: getUser\n      summary: Read one user\n      responses:\n        '200':\n          description: OK\n",
    )
    .expect("write OpenAPI fixture");
    let generated = out_dir.join("generated");

    let code = run(
        CliCommand {
            verb: Some("api".to_string()),
            args: vec![
                "import".to_string(),
                input.display().to_string(),
                "--module".to_string(),
                "external.Billing".to_string(),
                "--out".to_string(),
                generated.display().to_string(),
            ],
        },
        CliState::default(),
    );

    assert_eq!(code, ExitCode::SUCCESS);
    let module =
        fs::read_to_string(generated.join("billing.terl")).expect("read generated client module");
    assert!(module.contains("module external.Billing."));
    assert!(module.contains("Read one user"));
    assert!(module.contains("pub get_user_method(): String ->\n    \"GET\"."));
    assert!(module.contains("pub get_user_path(): String ->\n    \"/users/{id}\"."));

    let skip_manifest = fs::read_to_string(generated.join("api-import-skips.json"))
        .expect("read import skip manifest");
    assert!(skip_manifest.contains("\"schema\": \"terlan-api-import-skips-v1\""));
    assert!(skip_manifest.contains("\"skips\": []"));
}

/// Records unsupported OpenAPI features during import.
///
/// Inputs:
/// - One OpenAPI YAML fixture containing a `trace` operation.
///
/// Output:
/// - Test passes when the unsupported operation is listed in the skip manifest.
///
/// Transformation:
/// - Verifies import is explicit about features this slice cannot represent.
#[test]
fn api_import_records_unsupported_operation_skips() {
    let out_dir = temp_dir("api_command", "import_skip_outputs");
    let input = out_dir.join("openapi.yaml");
    fs::write(
        &input,
        "openapi: 3.0.3\ninfo:\n  title: Probe\n  version: 0.0.1\npaths:\n  /probe:\n    trace:\n      responses:\n        '200':\n          description: OK\n",
    )
    .expect("write OpenAPI fixture");
    let generated = out_dir.join("generated");

    assert_eq!(
        run(
            CliCommand {
                verb: Some("api".to_string()),
                args: vec![
                    "import".to_string(),
                    input.display().to_string(),
                    "--module".to_string(),
                    "external.Probe".to_string(),
                    "--out".to_string(),
                    generated.display().to_string(),
                ],
            },
            CliState::default(),
        ),
        ExitCode::SUCCESS
    );

    let skip_manifest = fs::read_to_string(generated.join("api-import-skips.json"))
        .expect("read import skip manifest");
    assert!(skip_manifest.contains("\"path\": \"/probe\""));
    assert!(skip_manifest.contains("TRACE operations are not imported in this slice"));
}

/// Writes a router source fixture for API command tests.
///
/// Inputs:
/// - `path`: destination file.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Produces a normal `std.http.Router` module with static and receiver-style
///   route builder calls.
fn write_router_source(path: &std::path::Path) {
    fs::write(
        path,
        "module app.Http.\n\nimport std.http.Router.\nimport std.http.Response.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub router(): Router ->\n    let router = Router.get(Router.new(), \"/\", home);\n    router.get(\"/users/:id\", show_user).\n\npub home(_request: Request): Response ->\n    Response.text(\"home\").\n\npub show_user(_request: Request): Response ->\n    Response.text(\"user\").\n",
    )
    .expect("write router source fixture");
}
