//! Pure operation dispatcher for Rust-backed NativeBoundary std adapters.
//!
//! This module is the first shared execution surface between compiler-native
//! operation ids such as `std.data.json.parse` and concrete Rust adapter
//! functions. The VM/native worker layer can call this module after it has
//! decoded runtime terms into `NativeBoundaryValue`.

use crate::terlan_native::{
    base64, hash as native_hash, http, json, md5, path, postgres, regex, toml, uri,
};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;

mod archive;
mod args;
mod arity;
mod filesystem;
mod git;
#[path = "dispatch/hash.rs"]
mod hash;
#[path = "dispatch/json.rs"]
mod json_dispatch;
mod manifest;
mod panic_boundary;
mod platform_dispatch;
mod process;
mod resources;

use args::{
    cookie_options_from_args, dispatch_base64_error, dispatch_http_error, dispatch_path_error,
    dispatch_postgres_error, dispatch_uri_error, expect_bool, expect_bytes, expect_http_cookie_jar,
    expect_http_request, expect_int, expect_json, expect_json_list, expect_path,
    expect_postgres_config, expect_postgres_pool, expect_postgres_row, expect_text, expect_uri,
    unknown_operation,
};
pub use arity::{operation_arity, validate_operation_arity};
use filesystem::{
    copy_directory_tree_excluding, create_directory_symbolic_link, create_temporary_directory,
    directory_entries, directory_files_recursive, directory_find_named_recursive_excluding,
    directory_tree_usage, dispatch_direct_file_operation, dispatch_directory_error,
    dispatch_file_error, expect_text_list, normalized_host_path, text_files_recursive,
    text_files_recursive_matching,
};
pub use resources::{
    dispatch_with_resources, dispatch_with_resources_for_process,
    dispatch_with_resources_for_process_with_capabilities,
    dispatch_with_resources_for_process_with_policy,
    dispatch_with_resources_for_process_with_policy_and_cancellation,
};

/// Neutral value shape accepted and returned by NativeBoundary adapter dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan VM-owned `Bytes`.
    Bytes(Vec<u8>),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Terlan atom identity without a host-language enum escape hatch.
    Atom(String),
    /// Descriptor-checked Terlan record/constructor value.
    Record {
        /// Constructor or record name.
        name: String,
        /// Ordered named fields.
        fields: Vec<(String, NativeBoundaryValue)>,
    },
    /// Ordered recursively owned Terlan values.
    List(Vec<NativeBoundaryValue>),
    /// Opaque `std.data.Json.Json`.
    Json(json::Json),
    /// Opaque compiled `std.regex.Regex.Regex`.
    Regex(regex::Regex),
    /// Opaque `std.http.Request.Request`.
    HttpRequest(http::Request),
    /// Opaque `std.http.Response.Response`.
    HttpResponse(http::Response),
    /// Opaque `std.http.Cookies.Jar`.
    HttpCookieJar(http::CookieJar),
    /// Opaque `std.io.Path.Path`.
    Path(path::Path),
    /// Opaque `std.net.Uri.Uri`.
    Uri(uri::Uri),
    /// Opaque `std.db.Postgres.Config`.
    PostgresConfig(postgres::Config),
    /// Opaque `std.db.Postgres.Pool`.
    PostgresPool(postgres::Pool),
    /// Opaque `std.db.Postgres.Row`.
    PostgresRow(postgres::Row),
    /// `List[std.data.Json.Json]` used for Postgres parameter values.
    JsonList(Vec<json::Json>),
    /// `List[std.db.Postgres.Row]` returned by Postgres query operations.
    PostgresRows(Vec<postgres::Row>),
    /// `Option[std.db.Postgres.Row]` returned by single-row Postgres queries.
    OptionalPostgresRow(Option<postgres::Row>),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Path]` for path component accessors.
    OptionalPath(Option<path::Path>),
}

/// Bridge-facing value shape that carries opaque resources as handles.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryBridgeValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan VM-owned `Bytes`.
    Bytes(Vec<u8>),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Terlan atom identity.
    Atom(String),
    /// Recursively owned Terlan record/constructor value.
    Record {
        /// Constructor or record name.
        name: String,
        /// Ordered named fields.
        fields: Vec<(String, NativeBoundaryBridgeValue)>,
    },
    /// Opaque resource handle for JSON, path, URI, or later native resources.
    Handle(NativeBoundaryHandle),
    /// Structured Postgres connection configuration for `connect`.
    PostgresConfig(postgres::Config),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Handle]` for optional opaque resources such as path parents.
    OptionalHandle(Option<NativeBoundaryHandle>),
    /// Terlan list carrying bridge-facing values.
    List(Vec<NativeBoundaryBridgeValue>),
}

/// Stable dispatcher error returned before crossing a runtime boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchError {
    code: &'static str,
    message: String,
    offset: usize,
    path: Option<String>,
}

impl DispatchError {
    /// Builds a dispatcher error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: source/input byte offset when available, or `0`.
    ///
    /// Output:
    /// - A `DispatchError` suitable for the NativeBoundary boundary.
    ///
    /// Transformation:
    /// - Stores adapter-independent error metadata without exposing backend
    ///   exception types.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
            path: None,
        }
    }

    /// Attaches structured filesystem path context to this error.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: dispatcher error.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns structured filesystem path context when the operation had one.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: dispatcher error.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the error.
    ///
    /// Inputs:
    /// - `self`: dispatcher error.
    ///
    /// Output:
    /// - Byte offset, or `0` when no adapter supplied one.
    ///
    /// Transformation:
    /// - Reads the offset field without allocation or mutation.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Dispatches one compiler-native operation to a NativeBoundary adapter function.
///
/// Inputs:
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: neutral runtime values decoded by the native bridge.
///
/// Output:
/// - `Ok(NativeBoundaryValue)` with the adapter result.
/// - `Err(DispatchError)` for unknown operation ids, arity mismatches, type
///   mismatches, or adapter-specific stable errors.
///
/// Transformation:
/// - Validates the operation id and argument shapes, calls the corresponding
///   Rust adapter, and converts adapter-specific errors into one dispatch
///   error shape.
pub fn dispatch(
    operation: &str,
    args: &[NativeBoundaryValue],
) -> Result<NativeBoundaryValue, DispatchError> {
    validate_arity(operation, args)?;
    if operation.starts_with("std.data.json.") {
        return json_dispatch::dispatch(operation, args);
    }
    if operation.starts_with("std.system.platform.") {
        return platform_dispatch::dispatch(operation);
    }
    match operation {
        "std.data.toml.parse" => toml::parse(expect_text(operation, args, 0)?)
            .map(NativeBoundaryValue::Json)
            .map_err(args::dispatch_json_error),
        "std.regex.regex.compile" => {
            let pattern = expect_text(operation, args, 0)?;
            regex::compile(pattern)
                .map(NativeBoundaryValue::Regex)
                .map_err(args::dispatch_regex_error)
        }
        "std.regex.regex.is_match" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::Bool(regex::is_match(value, text)))
        }
        "std.regex.regex.matching_line_numbers" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::List(
                regex::matching_line_numbers(value, text)
                    .into_iter()
                    .map(NativeBoundaryValue::Int)
                    .collect(),
            ))
        }
        "std.regex.regex.find" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(regex::find(value, text)))
        }
        "std.regex.regex.find_all" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::List(
                regex::find_all(value, text)
                    .into_iter()
                    .map(NativeBoundaryValue::Text)
                    .collect(),
            ))
        }
        "std.regex.regex.capture" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            let index = usize::try_from(expect_int(operation, args, 2)?)
                .map_err(|_| args::type_error(operation, 2, "nonnegative Int"))?;
            Ok(NativeBoundaryValue::OptionalText(regex::capture(
                value, text, index,
            )))
        }
        "std.regex.regex.named_capture" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            let name = expect_text(operation, args, 2)?;
            Ok(NativeBoundaryValue::OptionalText(regex::named_capture(
                value, text, name,
            )))
        }
        "std.regex.regex.replace" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            let replacement = expect_text(operation, args, 2)?;
            Ok(NativeBoundaryValue::Text(regex::replace(
                value,
                text,
                replacement,
            )))
        }
        "std.regex.regex.split" => {
            let value = args::expect_regex(operation, args, 0)?;
            let text = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::List(
                regex::split(value, text)
                    .into_iter()
                    .map(NativeBoundaryValue::Text)
                    .collect(),
            ))
        }
        "std.regex.regex.escape" => {
            let text = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(regex::escape(text)))
        }
        "std.http.request.body_json" => {
            let request = expect_http_request(operation, args, 0)?;
            http::body_json(request)
                .map(NativeBoundaryValue::Json)
                .map_err(dispatch_http_error)
        }
        "std.http.request.body_text" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(http::body_text(request)))
        }
        "std.http.request.method" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(http::method(request)))
        }
        "std.http.request.path" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(http::path(request)))
        }
        "std.http.request.param" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(http::param(request, name)))
        }
        "std.http.request.query" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(http::query(request, name)))
        }
        "std.http.request.query_string" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(http::query_string(request)))
        }
        "std.http.request.header" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(http::request_header(
                request, name,
            )))
        }
        "std.http.request.cookie" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(http::cookie(request, name)))
        }
        "std.http.request.cookies" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(NativeBoundaryValue::HttpCookieJar(http::cookies(request)))
        }
        "std.http.cookies.get" => {
            let jar = expect_http_cookie_jar(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalText(jar.get(name)))
        }
        "std.http.cookies.set" | "std.http.cookies.delete" => Err(DispatchError::new(
            "dispatch.mutable_receiver_requires_resource_bridge",
            format!(
                "operation `{operation}` mutates a cookie jar and must use resource-backed bridge dispatch"
            ),
            0,
        )),
        "std.http.cookies.set_header" => {
            let name = expect_text(operation, args, 0)?;
            let value = expect_text(operation, args, 1)?;
            let path = expect_text(operation, args, 2)?;
            let http_only = expect_bool(operation, args, 3)?;
            let secure = expect_bool(operation, args, 4)?;
            http::set_header(name, value, path, http_only, secure)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_http_error)
        }
        "std.http.cookies.set_header_with_options" => {
            let name = expect_text(operation, args, 0)?;
            let value = expect_text(operation, args, 1)?;
            let options = cookie_options_from_args(operation, args)?;
            http::set_header_with_options(name, value, &options)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_http_error)
        }
        "std.http.cookies.delete_header" => {
            let name = expect_text(operation, args, 0)?;
            let path = expect_text(operation, args, 1)?;
            http::delete_header(name, path)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_http_error)
        }
        "std.http.response.json" => {
            let value = expect_json(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(NativeBoundaryValue::HttpResponse(http::json(value, status)))
        }
        "std.http.response.json_text" => {
            let value = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(NativeBoundaryValue::HttpResponse(http::json_text(
                value, status,
            )))
        }
        "std.http.response.text" => {
            let value = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(NativeBoundaryValue::HttpResponse(http::text(value, status)))
        }
        "std.http.response.html" => {
            let value = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(NativeBoundaryValue::HttpResponse(http::html(value, status)))
        }
        "std.http.response.file" => {
            let path = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            let content_type = expect_text(operation, args, 2)?;
            Ok(NativeBoundaryValue::HttpResponse(http::file(
                path,
                status,
                content_type,
            )))
        }
        "std.http.response.redirect" => {
            let location = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(NativeBoundaryValue::HttpResponse(http::redirect(
                location, status,
            )))
        }
        "std.http.response.stream" => Err(DispatchError::new(
            "dispatch.streaming_requires_vm",
            "std.http.Response.stream requires the Terlan VM runtime",
            0,
        )),
        "std.http.response.status"
        | "std.http.response.header"
        | "std.http.response.set_cookie_header"
        | "std.http.response.with_cookies" => Err(DispatchError::new(
            "dispatch.mutable_receiver_requires_direct_lowering",
            format!(
                "operation `{operation}` mutates a receiver and must use direct native lowering"
            ),
            0,
        )),
        "std.encoding.base64.encode" => {
            let text = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(base64::encode(text)))
        }
        "std.encoding.base64.decode" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode(text)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_base64_error)
        }
        "std.encoding.base64.encode_url" => {
            let text = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(base64::encode_url(text)))
        }
        "std.encoding.base64.decode_url" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode_url(text)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_base64_error)
        }
        "std.encoding.base64.encode_bytes" => {
            let bytes = expect_bytes(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(base64::encode_bytes(bytes)))
        }
        "std.encoding.base64.decode_bytes" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode_bytes(text)
                .map(NativeBoundaryValue::Bytes)
                .map_err(dispatch_base64_error)
        }
        "std.encoding.base64.encode_url_bytes" => {
            let bytes = expect_bytes(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(base64::encode_url_bytes(bytes)))
        }
        "std.encoding.base64.decode_url_bytes" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode_url_bytes(text)
                .map(NativeBoundaryValue::Bytes)
                .map_err(dispatch_base64_error)
        }
        "std.encoding.md5.digest" => {
            let text = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(md5::digest(text)))
        }
        "std.crypto.hash.sha256_framed" => {
            let fields = expect_text_list(operation, args, 0)?;
            native_hash::sha256_framed(&fields)
                .map(NativeBoundaryValue::Text)
                .ok_or_else(|| hash::field_too_large(operation))
        }
        "std.crypto.hash.sha256_domain_framed" => {
            let domain = expect_text(operation, args, 0)?;
            let fields = expect_text_list(operation, args, 1)?;
            native_hash::sha256_domain_framed(domain, &fields)
                .map(NativeBoundaryValue::Text)
                .ok_or_else(|| hash::field_too_large(operation))
        }
        "std.crypto.hash.sha256_nul_separated" => {
            let fields = filesystem::expect_text_list(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(
                native_hash::sha256_nul_separated(&fields),
            ))
        }
        "std.crypto.hash.sha256_bytes" => {
            let bytes = expect_bytes(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(native_hash::sha256_bytes(bytes)))
        }
        "std.crypto.hash.sha256_file" => {
            let path = expect_text(operation, args, 0)?;
            hash::sha256_file(operation, path).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.verify_sha256_manifest" => {
            let root = expect_text(operation, args, 0)?;
            let manifest = expect_text(operation, args, 1)?;
            hash::verify_sha256_manifest(operation, root, manifest).map(NativeBoundaryValue::Bool)
        }
        "std.crypto.hash.sha256_tree" => {
            let root = expect_text(operation, args, 0)?;
            hash::sha256_tree(operation, root).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.sha256_selected_files" => {
            let root = expect_text(operation, args, 0)?;
            let relative_paths = filesystem::expect_text_list(operation, args, 1)?;
            hash::sha256_selected_files(operation, root, &relative_paths)
                .map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.sha256_labeled_file_digests" => {
            hash::sha256_labeled_file_digests(operation, args).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.sha256_labeled_file_contents" => {
            hash::sha256_labeled_file_contents(operation, args).map(NativeBoundaryValue::Text)
        }
        "std.crypto.hash.audit_labeled_files" => {
            let forbidden_fragments = filesystem::expect_text_list(operation, args, 1)?;
            hash::audit_labeled_files(operation, args, &forbidden_fragments)
        }
        "std.crypto.hash.audit_labeled_file_patterns" => {
            let root = expect_text(operation, args, 0)?;
            let forbidden_fragments = filesystem::expect_text_list(operation, args, 2)?;
            hash::audit_labeled_file_patterns(operation, root, args, &forbidden_fragments)
        }
        "std.vcs.git.source_tree_identity" => {
            let root = expect_text(operation, args, 0)?;
            git::source_tree_identity(operation, root)
        }
        operation @ ("std.io.archive.create" | "std.io.archive.extract") => {
            archive::dispatch(operation, args)
        }
        "std.io.console.println" => {
            let text = expect_text(operation, args, 0)?;
            println!("{text}");
            Ok(NativeBoundaryValue::Unit)
        }
        "std.io.console.eprintln" => {
            let text = expect_text(operation, args, 0)?;
            eprintln!("{text}");
            Ok(NativeBoundaryValue::Unit)
        }
        "std.time.clock.unix_time_ns" => {
            let elapsed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| {
                    DispatchError::new(
                        "dispatch.clock_before_unix_epoch",
                        error.to_string(),
                        0,
                    )
                })?;
            let nanos = i64::try_from(elapsed.as_nanos()).map_err(|_| {
                DispatchError::new(
                    "dispatch.clock_overflow",
                    "Unix timestamp does not fit Terlan Int",
                    0,
                )
            })?;
            Ok(NativeBoundaryValue::Int(nanos))
        }
        "std.time.clock.monotonic_time_ns" => {
            static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            let nanos = ORIGIN.get_or_init(std::time::Instant::now).elapsed().as_nanos();
            let nanos = i64::try_from(nanos).map_err(|_| {
                DispatchError::new(
                    "dispatch.clock_overflow",
                    "monotonic timestamp does not fit Terlan Int",
                    0,
                )
            })?;
            Ok(NativeBoundaryValue::Int(nanos))
        }
        operation @ ("std.io.file.exists"
        | "std.io.file.read_text"
        | "std.io.file.read_bytes"
        | "std.io.file.size"
        | "std.io.file.timestamps"
        | "std.io.file.set_timestamps"
        | "std.io.file.is_executable"
        | "std.io.file.set_executable"
        | "std.io.file.copy"
        | "std.io.file.copy_many") => dispatch_direct_file_operation(operation, args),
        "std.io.file.read_text_many" => {
            let paths = expect_text_list(operation, args, 0)?;
            paths
                .into_iter()
                .map(|path| {
                    std::fs::read_to_string(path)
                        .map(|contents| NativeBoundaryValue::Record {
                            name: "TextFile".to_string(),
                            fields: vec![
                                (
                                    "path".to_string(),
                                    NativeBoundaryValue::Text(path.to_string()),
                                ),
                                (
                                    "contents".to_string(),
                                    NativeBoundaryValue::Text(contents),
                                ),
                            ],
                        })
                        .map_err(|error| dispatch_file_error(operation, path, error))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(NativeBoundaryValue::List)
        }
        "std.io.file.read_text_directory" => {
            let directory = expect_text(operation, args, 0)?;
            let mut paths = std::fs::read_dir(directory)
                .map_err(|error| dispatch_file_error(operation, directory, error))?
                .map(|entry| {
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| dispatch_file_error(operation, directory, error))
                })
                .collect::<Result<Vec<_>, _>>()?;
            paths.sort();
            let mut files = Vec::new();
            for path in paths {
                let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
                    dispatch_file_error(operation, &normalized_host_path(&path), error)
                })?;
                if !metadata.file_type().is_file() {
                    continue;
                }
                let normalized = normalized_host_path(&path);
                let contents = std::fs::read_to_string(&path)
                    .map_err(|error| dispatch_file_error(operation, &normalized, error))?;
                files.push(NativeBoundaryValue::Record {
                    name: "TextFile".to_string(),
                    fields: vec![
                        ("path".to_string(), NativeBoundaryValue::Text(normalized)),
                        (
                            "contents".to_string(),
                            NativeBoundaryValue::Text(contents),
                        ),
                    ],
                });
            }
            Ok(NativeBoundaryValue::List(files))
        }
        "std.io.file.read_text_tree_excluding" => {
            let path = expect_text(operation, args, 0)?;
            let exclusions = expect_text_list(operation, args, 1)?;
            text_files_recursive(path, &exclusions)
        }
        "std.io.file.read_text_tree_matching" => {
            let path = expect_text(operation, args, 0)?;
            let exclusions = expect_text_list(operation, args, 1)?;
            let suffixes = expect_text_list(operation, args, 2)?;
            let excluded_suffixes = expect_text_list(operation, args, 3)?;
            let offset = expect_int(operation, args, 4)?;
            let limit = expect_int(operation, args, 5)?;
            let offset = usize::try_from(offset).map_err(|_| {
                DispatchError::new("boundary.value", "offset must be nonnegative", 0)
            })?;
            let limit = usize::try_from(limit).map_err(|_| {
                DispatchError::new("boundary.value", "limit must be nonnegative", 0)
            })?;
            if limit == 0 {
                return Err(DispatchError::new(
                    "boundary.value",
                    "limit must be positive",
                    0,
                ));
            }
            text_files_recursive_matching(
                path,
                &exclusions,
                &suffixes,
                &excluded_suffixes,
                offset,
                limit,
            )
        }
        "std.io.file.write_text" => {
            let value = expect_text(operation, args, 0)?;
            let contents = expect_text(operation, args, 1)?;
            std::fs::write(value, contents)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_file_error(operation, value, error))
        }
        "std.io.file.append_text" => {
            use std::io::Write as _;
            let value = expect_text(operation, args, 0)?;
            let contents = expect_text(operation, args, 1)?;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(value)
                .and_then(|mut file| file.write_all(contents.as_bytes()))
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_file_error(operation, value, error))
        }
        "std.io.file.delete" => {
            let value = expect_text(operation, args, 0)?;
            std::fs::remove_file(value)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_file_error(operation, value, error))
        }
        "std.system.environment.contains" => {
            let key = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::Bool(std::env::var(key).is_ok()))
        }
        "std.system.environment.get" => {
            let key = expect_text(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(std::env::var(key).ok()))
        }
        "std.system.environment.current_directory" => std::env::current_dir()
            .map(|path| NativeBoundaryValue::Text(path.to_string_lossy().into_owned()))
            .map_err(|error| {
                DispatchError::new(
                    "system.environment.current_directory",
                    format!("Current directory is unavailable: {error}"),
                    0,
                )
            }),
        "std.system.process.limits" => Ok(process::process_limits()),
        "std.system.process.run" => process::run_process(args, None),
        "std.system.process.run_many" => process::run_process_many(args, None),
        "std.system.process.run_length_framed" => {
            process::run_process_length_framed(args, None)
        }
        "std.io.directory.entries" => {
            let path = expect_text(operation, args, 0)?;
            directory_entries(path).map(NativeBoundaryValue::List)
        }
        "std.io.directory.files_recursive" => {
            let path = expect_text(operation, args, 0)?;
            directory_files_recursive(path, &[]).map(NativeBoundaryValue::List)
        }
        "std.io.directory.files_recursive_excluding" => {
            let path = expect_text(operation, args, 0)?;
            let exclusions = expect_text_list(operation, args, 1)?;
            directory_files_recursive(path, &exclusions).map(NativeBoundaryValue::List)
        }
        "std.io.directory.find_named_recursive_excluding" => {
            let path = expect_text(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            let exclusions = expect_text_list(operation, args, 2)?;
            directory_find_named_recursive_excluding(path, name, &exclusions)
                .map(NativeBoundaryValue::List)
        }
        "std.io.directory.tree_usage" => {
            let path = expect_text(operation, args, 0)?;
            directory_tree_usage(path)
        }
        "std.io.directory.copy_tree_excluding" => {
            let source = expect_text(operation, args, 0)?;
            let destination = expect_text(operation, args, 1)?;
            let exclusions = expect_text_list(operation, args, 2)?;
            copy_directory_tree_excluding(source, destination, &exclusions)
                .map(|()| NativeBoundaryValue::Unit)
        }
        "std.io.directory.create_symbolic_link" => {
            let target = expect_text(operation, args, 0)?;
            let link_path = expect_text(operation, args, 1)?;
            create_directory_symbolic_link(target, link_path)
                .map(|()| NativeBoundaryValue::Unit)
        }
        "std.io.directory.create_all" => {
            let path = expect_text(operation, args, 0)?;
            std::fs::create_dir_all(path)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_directory_error(operation, path, error))
        }
        "std.io.directory.create_temporary" => {
            let prefix = expect_text(operation, args, 0)?;
            create_temporary_directory(prefix).map(NativeBoundaryValue::Text)
        }
        "std.io.directory.remove_all" => {
            let path = expect_text(operation, args, 0)?;
            std::fs::remove_dir_all(path)
                .map(|()| NativeBoundaryValue::Unit)
                .map_err(|error| dispatch_directory_error(operation, path, error))
        }
        "std.io.path.from_string" => {
            let text = expect_text(operation, args, 0)?;
            path::from_string(text)
                .map(NativeBoundaryValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.to_string" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(path::to_string(value)))
        }
        "std.io.path.join" => {
            let value = expect_path(operation, args, 0)?;
            let child = expect_text(operation, args, 1)?;
            path::join(value, child)
                .map(NativeBoundaryValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.file_name" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(path::file_name(value)))
        }
        "std.io.path.extension" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(path::extension(value)))
        }
        "std.io.path.parent" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalPath(path::parent(value)))
        }
        "std.io.path.is_absolute" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::Bool(path::is_absolute(value)))
        }
        "std.io.path.normalize" => {
            let value = expect_path(operation, args, 0)?;
            Ok(NativeBoundaryValue::Path(path::normalize(value)))
        }
        "std.io.path.starts_with" => {
            let value = expect_path(operation, args, 0)?;
            let base = expect_path(operation, args, 1)?;
            Ok(NativeBoundaryValue::Bool(path::starts_with(value, base)))
        }
        "std.io.path.strip_prefix" => {
            let value = expect_path(operation, args, 0)?;
            let base = expect_path(operation, args, 1)?;
            Ok(NativeBoundaryValue::OptionalPath(path::strip_prefix(
                value, base,
            )))
        }
        "std.net.uri.parse" => {
            let text = expect_text(operation, args, 0)?;
            uri::parse(text)
                .map(NativeBoundaryValue::Uri)
                .map_err(dispatch_uri_error)
        }
        "std.net.uri.to_string" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(uri::to_string(value)))
        }
        "std.net.uri.scheme" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(uri::scheme(value)))
        }
        "std.net.uri.host" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(uri::host(value)))
        }
        "std.net.uri.path" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(NativeBoundaryValue::Text(uri::path(value)))
        }
        "std.net.uri.query" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(uri::query(value)))
        }
        "std.net.uri.fragment" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(NativeBoundaryValue::OptionalText(uri::fragment(value)))
        }
        "std.db.postgres.connect" => {
            let config = expect_postgres_config(operation, args, 0)?;
            postgres::connect(config)
                .map(NativeBoundaryValue::PostgresPool)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.query" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::query(pool, sql, params)
                .map(NativeBoundaryValue::PostgresRows)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.query_one" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::query_one(pool, sql, params)
                .map(NativeBoundaryValue::OptionalPostgresRow)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.execute" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::execute(pool, sql, params)
                .map(NativeBoundaryValue::Int)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.transaction" => {
            let _pool = expect_postgres_pool(operation, args, 0)?;
            Err(DispatchError::new(
                "dispatch.callback_requires_runtime_bridge",
                "Postgres transaction callbacks require runtime bridge lowering.",
                0,
            ))
        }
        "std.db.postgres.string" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::string(row, name)
                .map(NativeBoundaryValue::Text)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.int" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::int(row, name)
                .map(NativeBoundaryValue::Int)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.bool" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::r#bool(row, name)
                .map(NativeBoundaryValue::Bool)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.json" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::json(row, name)
                .map(NativeBoundaryValue::Json)
                .map_err(dispatch_postgres_error)
        }
        _ => Err(unknown_operation(operation)),
    }
}

/// Validates argument count for one operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `args`: neutral runtime values supplied by the bridge.
///
/// Output:
/// - `Ok(())` when arity matches.
/// - `Err(DispatchError)` for unknown operations or wrong arity.
///
/// Transformation:
/// - Compares supplied argument count with `operation_arity`.
fn validate_arity(operation: &str, args: &[NativeBoundaryValue]) -> Result<(), DispatchError> {
    validate_operation_arity(operation, args.len(), unknown_operation)
}

#[cfg(test)]
#[path = "dispatch_test.rs"]
#[cfg(test)]
mod dispatch_test;
