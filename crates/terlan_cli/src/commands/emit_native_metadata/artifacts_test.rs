use super::*;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builds representative native metadata for artifact-rendering tests.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static SafeNative metadata with one native function signature.
///
/// Transformation:
/// - Constructs the smallest metadata object that exercises function-list
///   rendering and bridge skeleton generation.
fn sample_metadata() -> NativeMetadata {
    NativeMetadata {
        source_module: "app.Native".to_string(),
        native_module: "app_native_safe_native".to_string(),
        scheduler: "dirty_cpu".to_string(),
        native_policy: NativePolicy::SafeNativeOptional,
        functions: vec![NativeFunctionSignature {
            name: "work".to_string(),
            arity: 1,
            operation: None,
        }],
    }
}

/// Creates a unique temporary directory for artifact emission tests.
///
/// Inputs:
/// - `name`: stable test label included in the directory name.
///
/// Output:
/// - Filesystem path that does not exist before the test uses it.
///
/// Transformation:
/// - Combines process id and current timestamp to avoid collisions across
///   parallel test execution.
fn temp_output_dir(name: &str) -> std::path::PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "terlan_safe_native_{name}_{}_{}",
        std::process::id(),
        now
    ))
}

/// Returns the Rust-backed JSON std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.data.Json`.
///
/// Transformation:
/// - Embeds the real std module so metadata tests cover the release
///   contract instead of a synthetic duplicate.
fn json_std_source() -> &'static str {
    include_str!("../../../../../std/data/json.terl")
}

/// Returns the Rust-backed Base64 std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.encoding.Base64`.
///
/// Transformation:
/// - Embeds the real std module so SafeNative metadata extraction is
///   checked against the release-owned source.
fn base64_std_source() -> &'static str {
    include_str!("../../../../../std/encoding/base64.terl")
}

/// Returns the Rust-backed Path std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.io.Path`.
///
/// Transformation:
/// - Embeds the real std module so receiver-method operation arities are
///   checked against the release-owned source.
fn path_std_source() -> &'static str {
    include_str!("../../../../../std/io/path.terl")
}

/// Returns the Rust-backed URI std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.net.Uri`.
///
/// Transformation:
/// - Embeds the real std module so SafeNative metadata extraction is
///   checked against the release-owned source.
fn uri_std_source() -> &'static str {
    include_str!("../../../../../std/net/uri.terl")
}

/// Returns the Rust-backed HTTP request std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.http.Request`.
///
/// Transformation:
/// - Embeds the real std module so SafeNative metadata extraction is checked
///   against the release-owned request helper source.
fn http_request_std_source() -> &'static str {
    include_str!("../../../../../std/http/request.terl")
}

/// Returns the Rust-backed HTTP cookies std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.http.Cookies`.
///
/// Transformation:
/// - Embeds the real std module so SafeNative metadata extraction is checked
///   against the release-owned cookie helper source.
fn http_cookies_std_source() -> &'static str {
    include_str!("../../../../../std/http/cookies.terl")
}

/// Returns the Rust-backed HTTP response std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.http.Response`.
///
/// Transformation:
/// - Embeds the real std module so SafeNative metadata extraction is checked
///   against the release-owned response helper source.
fn http_response_std_source() -> &'static str {
    include_str!("../../../../../std/http/response.terl")
}

/// Returns the Rust-backed Postgres std source contract.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static source text for `std.db.Postgres`.
///
/// Transformation:
/// - Embeds the real std module so SafeNative metadata extraction is checked
///   against the release-owned database helper source.
fn postgres_std_source() -> &'static str {
    include_str!("../../../../../std/db/postgres.terl")
}

/// Asserts that metadata contains one native operation signature.
///
/// Inputs:
/// - `metadata`: extracted SafeNative metadata.
/// - `name`: expected Terlan function or method name.
/// - `arity`: expected backend arity, including receiver when present.
/// - `operation`: expected compiler-native operation id.
///
/// Output:
/// - Test assertion only.
///
/// Transformation:
/// - Converts expected parts into the same signature shape emitted by the
///   extractor and checks exact membership.
fn assert_operation(metadata: &NativeMetadata, name: &str, arity: usize, operation: &str) {
    assert!(metadata.functions.contains(&NativeFunctionSignature {
        name: name.to_string(),
        arity,
        operation: Some(operation.to_string()),
    }));
}

/// Verifies noncanonical native-core blocks are not artifact inputs.
///
/// Inputs:
/// - Source text containing the older `native core module` block shape.
///
/// Output:
/// - Test assertion over the extraction error.
///
/// Transformation:
/// - Runs metadata extraction directly and confirms the command artifact
///   path requires canonical `@compiler.native` declarations.
#[test]
fn native_metadata_rejects_native_core_module_without_compiler_native_annotations() {
    let source = r#"module native_meta.

pub length[T](items: List[T]): Int ->
    0.

native core module NativeArray {
    #[native(normal)]
    length[T](items: List[T]): Int.
}
"#;
    let err = extract_native_metadata(source, NativePolicy::SafeNativeOptional)
        .expect_err("native core module should not be a CLI artifact input");

    assert!(err.contains("@compiler.native"));
}

/// Verifies compiler-native annotations produce SafeNative metadata.
///
/// Inputs:
/// - Real `std.data.Json` source text.
///
/// Output:
/// - Test assertions over extracted metadata.
///
/// Transformation:
/// - Extracts metadata from `@compiler.native` annotations, derives the
///   backend module name, and confirms receiver-method arities include the
///   receiver argument.
#[test]
fn compiler_native_metadata_extracts_std_json_operations() {
    let metadata =
        extract_native_metadata(json_std_source(), NativePolicy::Pure).expect("metadata");

    assert_eq!(metadata.source_module, "std.data.Json");
    assert_eq!(metadata.native_module, "std_data_json_safe_native");
    assert_eq!(metadata.scheduler, "normal");
    assert_eq!(metadata.native_policy, NativePolicy::SafeNativeOptional);
    assert_eq!(metadata.functions.len(), 19);
    assert!(metadata.functions.contains(&NativeFunctionSignature {
        name: "parse".to_string(),
        arity: 1,
        operation: Some("std.data.json.parse".to_string()),
    }));
    assert!(metadata.functions.contains(&NativeFunctionSignature {
        name: "get".to_string(),
        arity: 2,
        operation: Some("std.data.json.get".to_string()),
    }));
    assert!(metadata.functions.contains(&NativeFunctionSignature {
        name: "length".to_string(),
        arity: 1,
        operation: Some("std.data.json.length".to_string()),
    }));
    assert!(metadata.functions.contains(&NativeFunctionSignature {
        name: "at".to_string(),
        arity: 2,
        operation: Some("std.data.json.at".to_string()),
    }));
    assert!(metadata.functions.contains(&NativeFunctionSignature {
        name: "is_null".to_string(),
        arity: 1,
        operation: Some("std.data.json.is_null".to_string()),
    }));
}

/// Verifies source overloads sharing one backend operation emit one native row.
///
/// Inputs:
/// - A tiny module with two same-name same-arity native overload declarations
///   mapped to the same operation id.
///
/// Output:
/// - Extracted metadata with one deduplicated operation signature.
///
/// Transformation:
/// - Exercises SafeNative metadata extraction before JSON/Rust rendering so
///   overloaded source APIs do not produce duplicate backend dispatch arms.
#[test]
fn compiler_native_metadata_deduplicates_identical_overload_operations() {
    let source = "\
module native.Dedupe.\n\
\n\
@compiler.native {native.dedupe.html}\n\
pub html(value: String, status: Int = 200): Response ->\n\
    native.\n\
\n\
@compiler.native {native.dedupe.html}\n\
pub html(value: Html, status: Int = 200): Response ->\n\
    native.\n\
";

    let metadata = extract_native_metadata(source, NativePolicy::Pure).expect("metadata");

    assert_eq!(
        metadata.functions,
        vec![NativeFunctionSignature {
            name: "html".to_string(),
            arity: 2,
            operation: Some("native.dedupe.html".to_string()),
        }]
    );
}

/// Verifies every Rust-backed std module enters SafeNative metadata.
///
/// Inputs:
/// - Real source contracts for JSON, Base64, Path, URI, and HTTP.
///
/// Output:
/// - Test assertions over derived module names and operation signatures.
///
/// Transformation:
/// - Extracts metadata from each release-owned source file and checks the
///   operation inventory expected by `std/RUST_BACKED_MANIFEST.tsv`.
#[test]
fn compiler_native_metadata_extracts_all_rust_backed_std_operations() {
    let cases: [(&str, &str, &str, usize, &[(&str, usize, &str)]); 8] = [
        (
            "std.data.Json",
            json_std_source(),
            "std_data_json_safe_native",
            19,
            &[
                ("null", 0, "std.data.json.null"),
                ("bool", 1, "std.data.json.bool"),
                ("int", 1, "std.data.json.int"),
                ("float", 1, "std.data.json.float"),
                ("string", 1, "std.data.json.string"),
                ("array", 0, "std.data.json.array"),
                ("object", 0, "std.data.json.object"),
                ("push", 2, "std.data.json.array_push"),
                ("put", 3, "std.data.json.object_put"),
                ("parse", 1, "std.data.json.parse"),
                ("stringify", 1, "std.data.json.stringify"),
                ("get", 2, "std.data.json.get"),
                ("length", 1, "std.data.json.length"),
                ("at", 2, "std.data.json.at"),
                ("as_string", 1, "std.data.json.as_string"),
                ("as_int", 1, "std.data.json.as_int"),
                ("as_float", 1, "std.data.json.as_float"),
                ("as_bool", 1, "std.data.json.as_bool"),
                ("is_null", 1, "std.data.json.is_null"),
            ],
        ),
        (
            "std.encoding.Base64",
            base64_std_source(),
            "std_encoding_base64_safe_native",
            4,
            &[
                ("encode", 1, "std.encoding.base64.encode"),
                ("decode", 1, "std.encoding.base64.decode"),
                ("encode_url", 1, "std.encoding.base64.encode_url"),
                ("decode_url", 1, "std.encoding.base64.decode_url"),
            ],
        ),
        (
            "std.io.Path",
            path_std_source(),
            "std_io_path_safe_native",
            7,
            &[
                ("from_string", 1, "std.io.path.from_string"),
                ("to_string", 1, "std.io.path.to_string"),
                ("join", 2, "std.io.path.join"),
                ("file_name", 1, "std.io.path.file_name"),
                ("extension", 1, "std.io.path.extension"),
                ("parent", 1, "std.io.path.parent"),
                ("is_absolute", 1, "std.io.path.is_absolute"),
            ],
        ),
        (
            "std.net.Uri",
            uri_std_source(),
            "std_net_uri_safe_native",
            7,
            &[
                ("parse", 1, "std.net.uri.parse"),
                ("to_string", 1, "std.net.uri.to_string"),
                ("scheme", 1, "std.net.uri.scheme"),
                ("host", 1, "std.net.uri.host"),
                ("path", 1, "std.net.uri.path"),
                ("query", 1, "std.net.uri.query"),
                ("fragment", 1, "std.net.uri.fragment"),
            ],
        ),
        (
            "std.http.Request",
            http_request_std_source(),
            "std_http_request_safe_native",
            10,
            &[
                ("method", 1, "std.http.request.method"),
                ("path", 1, "std.http.request.path"),
                ("param", 2, "std.http.request.param"),
                ("query", 2, "std.http.request.query"),
                ("query_string", 1, "std.http.request.query_string"),
                ("header", 2, "std.http.request.header"),
                ("cookie", 2, "std.http.request.cookie"),
                ("cookies", 1, "std.http.request.cookies"),
                ("body_text", 1, "std.http.request.body_text"),
                ("body_json", 1, "std.http.request.body_json"),
            ],
        ),
        (
            "std.http.Cookies",
            http_cookies_std_source(),
            "std_http_cookies_safe_native",
            6,
            &[
                ("get", 2, "std.http.cookies.get"),
                ("set", 6, "std.http.cookies.set"),
                ("delete", 3, "std.http.cookies.delete"),
                ("set_header", 5, "std.http.cookies.set_header"),
                (
                    "set_header_with_options",
                    10,
                    "std.http.cookies.set_header_with_options",
                ),
                ("delete_header", 2, "std.http.cookies.delete_header"),
            ],
        ),
        (
            "std.http.Response",
            http_response_std_source(),
            "std_http_response_safe_native",
            9,
            &[
                ("json", 2, "std.http.response.json"),
                ("json_text", 2, "std.http.response.json_text"),
                ("text", 2, "std.http.response.text"),
                ("html", 2, "std.http.response.html"),
                ("file", 3, "std.http.response.file"),
                ("redirect", 2, "std.http.response.redirect"),
                ("status", 2, "std.http.response.status"),
                ("header", 3, "std.http.response.header"),
                (
                    "set_cookie_header",
                    2,
                    "std.http.response.set_cookie_header",
                ),
            ],
        ),
        (
            "std.db.Postgres",
            postgres_std_source(),
            "std_db_postgres_safe_native",
            9,
            &[
                ("connect", 1, "std.db.postgres.connect"),
                ("query", 3, "std.db.postgres.query"),
                ("query_one", 3, "std.db.postgres.query_one"),
                ("execute", 3, "std.db.postgres.execute"),
                ("transaction", 2, "std.db.postgres.transaction"),
                ("string", 2, "std.db.postgres.string"),
                ("int", 2, "std.db.postgres.int"),
                ("bool", 2, "std.db.postgres.bool"),
                ("json", 2, "std.db.postgres.json"),
            ],
        ),
    ];

    for (source_module, source, native_module, operation_count, operations) in cases {
        let metadata = extract_native_metadata(source, NativePolicy::Pure).expect(source_module);
        assert_eq!(metadata.source_module, source_module);
        assert_eq!(metadata.native_module, native_module);
        assert_eq!(metadata.scheduler, "normal");
        assert_eq!(metadata.native_policy, NativePolicy::SafeNativeOptional);
        assert_eq!(metadata.functions.len(), operation_count);
        for (name, arity, operation) in operations {
            assert_operation(&metadata, name, *arity, operation);
        }
    }
}

/// Verifies artifact emission works for compiler-native std modules.
///
/// Inputs:
/// - Real `std.data.Json` source text and a temporary output directory.
///
/// Output:
/// - Filesystem and metadata assertions.
///
/// Transformation:
/// - Emits SafeNative artifacts from compiler-native annotations and checks
///   that the generated JSON and Rust stub preserve operation ids.
#[test]
fn emit_native_artifacts_writes_compiler_native_std_files() {
    let out_dir = temp_output_dir("compiler_native_std");

    emit_native_artifacts(
        json_std_source(),
        &out_dir,
        NativePolicy::SafeNativeOptional,
        false,
    )
    .expect("compiler-native safe native artifacts should emit");

    let metadata_path = out_dir.join("std.data.Json.safe_native.json");
    let rust_stub_path = out_dir.join("std_data_json_safe_native.safe_native.rs");
    assert!(metadata_path.exists());
    assert!(rust_stub_path.exists());

    let metadata = fs::read_to_string(metadata_path).expect("read metadata");
    let rust_stub = fs::read_to_string(rust_stub_path).expect("read rust stub");
    assert!(metadata.contains("\"operation\": \"std.data.json.parse\""));
    assert!(rust_stub.contains("pub const OPERATIONS"));
    assert!(rust_stub.contains("(\"parse\", \"std.data.json.parse\", 1)"));
    assert!(rust_stub.contains("(\"get\", \"std.data.json.get\", 2)"));

    fs::remove_dir_all(out_dir).expect("remove emitted artifacts");
}

/// Serializes native metadata through serde-backed JSON generation.
///
/// Inputs:
/// - Native metadata containing string content with JSON-sensitive characters.
///
/// Output:
/// - Test passes when the emitted metadata parses as JSON and preserves the
///   original string values.
///
/// Transformation:
/// - Guards against regressions to hand-built JSON string interpolation in
///   SafeNative metadata.
#[test]
fn native_metadata_to_json_round_trips_escaped_strings() {
    let metadata = NativeMetadata {
        source_module: "std.data.Json\nSource".to_string(),
        native_module: "std.data.Json\"Native".to_string(),
        scheduler: "safe\\native".to_string(),
        native_policy: NativePolicy::SafeNativeOptional,
        functions: vec![NativeFunctionSignature {
            name: "parse\tvalue".to_string(),
            arity: 1,
            operation: Some("std.data.json.parse\u{0008}".to_string()),
        }],
    };

    let parsed: serde_json::Value =
        serde_json::from_str(&metadata.to_json()).expect("metadata json parses");

    assert_eq!(parsed["source_module"], "std.data.Json\nSource");
    assert_eq!(parsed["module"], "std.data.Json\"Native");
    assert_eq!(parsed["scheduler"], "safe\\native");
    assert_eq!(parsed["functions"][0]["name"], "parse\tvalue");
    assert_eq!(
        parsed["functions"][0]["operation"],
        "std.data.json.parse\u{0008}"
    );
}

/// Verifies the generated Rust SafeNative stub carries the bridge contract.
///
/// Inputs:
/// - Representative native metadata.
///
/// Output:
/// - Test assertions over generated source text.
///
/// Transformation:
/// - Renders the stub and checks for opaque handles, typed replies,
///   request ids, credit reporting, explicit disposal, and stale-handle
///   errors.
#[test]
fn safe_native_rust_stub_contains_actor_bridge_contract() {
    let stub = emit_safe_native_rust_stub(&sample_metadata());

    assert!(stub.contains("pub struct SafeNativeHandle"));
    assert!(stub.contains("pub struct SafeNativeReply"));
    assert!(stub.contains("pub struct SafeNativeWorker"));
    assert!(stub.contains("Text(String)"));
    assert!(stub.contains("Int(i64)"));
    assert!(stub.contains("Float(f64)"));
    assert!(stub.contains("Bool(bool)"));
    assert!(stub.contains("OptionalText(Option<String>)"));
    assert!(stub.contains("OptionalHandle(Option<SafeNativeHandle>)"));
    assert!(stub.contains("request_id: u64"));
    assert!(stub.contains("credits: usize"));
    assert!(stub.contains("offset: usize"));
    assert!(stub.contains("Register { request_id"));
    assert!(stub.contains("Call { request_id"));
    assert!(stub.contains("args: Vec<SafeNativeValue>"));
    assert!(stub.contains("validate_args(&resources, &args)"));
    assert!(stub.contains("SafeNativeValue::OptionalHandle(Some(handle))"));
    assert!(stub.contains("native_operation_unimplemented"));
    assert!(stub.contains("native_operation_unknown"));
    assert!(stub.contains("\"work\" => native_unimplemented_operation(operation)"));
    assert!(stub.contains("Dispose { request_id"));
    assert!(stub.contains("stale_native_handle"));
    assert!(stub.contains("DEFAULT_CREDIT_WINDOW"));
}

/// Verifies the generated Rust SafeNative stub passes unsafe-pattern checks.
///
/// Inputs:
/// - Representative native metadata.
///
/// Output:
/// - Test assertion over validator success.
///
/// Transformation:
/// - Renders the same stub used by artifact emission and runs the
///   conservative SafeNative unsafe-pattern scanner.
#[test]
fn safe_native_rust_stub_satisfies_validator() {
    let stub = emit_safe_native_rust_stub(&sample_metadata());

    validate_safe_native_rust_stub(&stub).expect("generated stub should satisfy validator");
}

/// Verifies the generated Rust SafeNative stub compiles as a library.
///
/// Inputs:
/// - Representative native metadata and a temporary Rust source path.
///
/// Output:
/// - Test passes when `rustc` accepts the generated skeleton.
///
/// Transformation:
/// - Writes the generated stub to a temporary `.rs` file, compiles it with
///   an explicit crate name, and reports compiler stderr on failure.
#[test]
fn safe_native_rust_stub_compiles_as_library() {
    let out_dir = temp_output_dir("safe_native_rust_stub_compile");
    let stub_path = out_dir.join("safe_native_stub.rs");
    let output_path = out_dir.join("safe_native_stub.rlib");
    fs::create_dir_all(&out_dir).expect("create generated rustc test directory");
    fs::write(&stub_path, emit_safe_native_rust_stub(&sample_metadata()))
        .expect("write generated safe native rust stub");

    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| std::ffi::OsString::from("rustc"));
    let output = Command::new(rustc)
        .args([
            "--crate-type",
            "lib",
            "--crate-name",
            "safe_native_stub_check",
        ])
        .arg(&stub_path)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("run rustc for generated safe native rust stub");

    assert!(
        output.status.success(),
        "rustc failed for generated SafeNative stub:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(out_dir).expect("remove generated rustc test directory");
}

/// Verifies the generated Erlang loader uses the neutral SafeNative path.
///
/// Inputs:
/// - Representative native metadata.
///
/// Output:
/// - Test assertions over generated Erlang source text.
///
/// Transformation:
/// - Renders the BEAM loader stub and confirms the public environment
///   variable uses SafeNative naming rather than NIF-specific naming.
#[test]
fn safe_native_erl_stub_uses_neutral_loader_env_var() {
    let stub = emit_safe_native_erl_stub(&sample_metadata());

    assert!(stub.contains("TERLAN_SAFE_NATIVE_PATH"));
    assert!(!stub.contains("TERLAN_SAFE_NIF_PATH"));
    assert!(!stub.contains("erlang:load_nif"));
    assert!(!stub.contains("erlang:nif_error"));
    assert!(!stub.contains("nif_not_loaded"));
}

/// Verifies the generated Erlang loader exposes the worker transport ABI.
///
/// Inputs:
/// - Representative native metadata.
///
/// Output:
/// - Test assertions over generated Erlang source text.
///
/// Transformation:
/// - Renders the BEAM loader stub and checks for stable metadata,
///   operation inventory, and worker command placeholder exports.
#[test]
fn safe_native_erl_stub_contains_worker_transport_contract() {
    let stub = emit_safe_native_erl_stub(&sample_metadata());

    assert!(stub.contains("-export([load/0, metadata/0, operations/0])."));
    assert!(
        stub.contains("-export([start_worker/1, call_worker/3, dispose_worker/2, stop_worker/1]).")
    );
    assert!(stub.contains("metadata() ->"));
    assert!(stub.contains("source_module => <<\"app.Native\">>"));
    assert!(stub.contains("native_module => <<\"app_native_safe_native\">>"));
    assert!(stub.contains("operations() ->"));
    assert!(stub.contains("{<<\"work\">>, <<\"work\">>, 1}"));
    assert!(stub.contains("start_worker(Options) ->"));
    assert!(stub.contains("call_worker(RequestId, Operation, Args)"));
    assert!(stub.contains("dispose_worker(RequestId, _Handle)"));
    assert!(stub.contains("stop_worker(_Bridge) ->"));
    assert!(stub.contains("safe_native_not_loaded_error() ->"));
    assert!(stub.contains("safe_native.not_loaded"));
    assert!(stub.contains("work(A1) ->\n    call_operation(<<\"work\">>, [A1])."));
    assert!(stub.contains("{safe_native_reply, RequestId, {error, Error}, 0}"));
    assert!(stub.contains("[<<\"result_ok_string\">>, Value]"));
    assert!(stub.contains("[<<\"result_ok_int\">>, Value]"));
}

/// Verifies the generated Erlang SafeNative loader compiles.
///
/// Inputs:
/// - Representative native metadata and a temporary Erlang source path.
///
/// Output:
/// - Test passes when `erlc` accepts the generated loader module.
///
/// Transformation:
/// - Writes the generated loader to a temporary `.erl` file, compiles it
///   into the same directory, and reports compiler output on failure.
#[test]
fn safe_native_erl_stub_compiles_as_module() {
    let metadata = sample_metadata();
    let out_dir = temp_output_dir("safe_native_erl_stub_compile");
    fs::create_dir_all(&out_dir).expect("create generated erlc test directory");
    let stub_path = out_dir.join(format!("{}.erl", metadata.native_module));
    fs::write(&stub_path, emit_safe_native_erl_stub(&metadata))
        .expect("write generated safe native erlang stub");

    let erlc = std::env::var_os("ERLC").unwrap_or_else(|| std::ffi::OsString::from("erlc"));
    let output = Command::new(erlc)
        .arg("-o")
        .arg(&out_dir)
        .arg(&stub_path)
        .output()
        .expect("run erlc for generated safe native erlang stub");

    assert!(
        output.status.success(),
        "erlc failed for generated SafeNative stub:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(out_dir).expect("remove generated erlc test directory");
}

/// Verifies the generated Erlang loader metadata runs without a native path.
///
/// Inputs:
/// - Representative native metadata and a temporary Erlang build directory.
///
/// Output:
/// - Test passes when `erl` can call `metadata/0` and `operations/0`.
///
/// Transformation:
/// - Compiles the generated loader, removes the SafeNative library path
///   environment variable, loads the BEAM module in a VM, and checks the
///   runtime output for the expected metadata and operation inventory.
#[test]
fn safe_native_erl_stub_metadata_runs_without_native_library() {
    let metadata = sample_metadata();
    let out_dir = temp_output_dir("safe_native_erl_stub_runtime");
    fs::create_dir_all(&out_dir).expect("create generated erl runtime test directory");
    let stub_path = out_dir.join(format!("{}.erl", metadata.native_module));
    fs::write(&stub_path, emit_safe_native_erl_stub(&metadata))
        .expect("write generated safe native erlang stub");

    let erlc = std::env::var_os("ERLC").unwrap_or_else(|| std::ffi::OsString::from("erlc"));
    let compile_output = Command::new(erlc)
        .arg("-o")
        .arg(&out_dir)
        .arg(&stub_path)
        .output()
        .expect("run erlc for generated safe native erlang stub");
    assert!(
        compile_output.status.success(),
        "erlc failed for generated SafeNative stub:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compile_output.stdout),
        String::from_utf8_lossy(&compile_output.stderr)
    );

    let eval = format!(
            "M = {}:metadata(), Ops = {}:operations(), Reply = {}:call_worker(7, <<\"work\">>, []), io:format(\"~p~n~p~n~p~n\", [M, Ops, Reply]), halt().",
            metadata.native_module, metadata.native_module, metadata.native_module
        );
    let erl = std::env::var_os("ERL").unwrap_or_else(|| std::ffi::OsString::from("erl"));
    let runtime_output = Command::new(erl)
        .arg("-noshell")
        .arg("-pa")
        .arg(&out_dir)
        .arg("-eval")
        .arg(eval)
        .env_remove("TERLAN_SAFE_NATIVE_PATH")
        .output()
        .expect("run erl for generated safe native erlang stub");
    assert!(
        runtime_output.status.success(),
        "erl failed for generated SafeNative stub:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&runtime_output.stdout),
        String::from_utf8_lossy(&runtime_output.stderr)
    );
    let stdout = String::from_utf8_lossy(&runtime_output.stdout);
    assert!(stdout.contains("source_module => <<\"app.Native\">>"));
    assert!(stdout.contains("native_module => <<\"app_native_safe_native\">>"));
    assert!(stdout.contains("{<<\"work\">>,<<\"work\">>,1}"));
    assert!(stdout.contains("{safe_native_reply,7"));
    assert!(stdout.contains("safe_native_not_loaded"));

    fs::remove_dir_all(out_dir).expect("remove generated erl runtime test directory");
}

/// Verifies emitted SafeNative files use the neutral artifact names.
///
/// Inputs:
/// - Real `std.data.Json` source and a temporary output directory.
///
/// Output:
/// - Filesystem assertions only.
///
/// Transformation:
/// - Emits artifacts directly and confirms generated filenames no longer
///   expose the older NIF-specific `safe_nif` label.
#[test]
fn emit_native_artifacts_writes_safe_native_filenames() {
    let out_dir = temp_output_dir("filenames");

    emit_native_artifacts(
        json_std_source(),
        &out_dir,
        NativePolicy::SafeNativeOptional,
        false,
    )
    .expect("safe native artifacts should emit");

    assert!(out_dir.join("std.data.Json.safe_native.json").exists());
    assert!(out_dir
        .join("std_data_json_safe_native.safe_native.rs")
        .exists());
    assert!(!out_dir.join("std.data.Json.safe_nif.json").exists());
    assert!(!out_dir
        .join("std_data_json_safe_native.safe_nif.rs")
        .exists());

    fs::remove_dir_all(out_dir).expect("remove emitted artifacts");
}
