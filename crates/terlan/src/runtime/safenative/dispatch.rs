//! Pure operation dispatcher for Rust-backed SafeNative std adapters.
//!
//! This module is the first shared execution surface between compiler-native
//! operation ids such as `std.data.json.parse` and concrete Rust adapter
//! functions. The BEAM/native worker layer can call this module after it has
//! decoded runtime terms into `SafeNativeValue`.

use crate::terlan_native::{base64, http, json, path, postgres, uri};
use crate::terlan_safenative::handle::SafeNativeHandle;

mod args;
mod arity;
mod resources;

use args::{
    cookie_options_from_args, dispatch_base64_error, dispatch_http_error, dispatch_json_error,
    dispatch_path_error, dispatch_postgres_error, dispatch_uri_error, expect_bool, expect_float,
    expect_http_cookie_jar, expect_http_request, expect_int, expect_json, expect_json_list,
    expect_path, expect_postgres_config, expect_postgres_pool, expect_postgres_row, expect_text,
    expect_uri, unknown_operation,
};
pub use arity::{operation_arity, validate_operation_arity};
pub use resources::dispatch_with_resources;

/// Neutral value shape accepted and returned by SafeNative adapter dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Opaque `std.data.Json.Json`.
    Json(json::Json),
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
pub enum SafeNativeBridgeValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Opaque resource handle for JSON, path, URI, or later native resources.
    Handle(SafeNativeHandle),
    /// Structured Postgres connection configuration for `connect`.
    PostgresConfig(postgres::Config),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Handle]` for optional opaque resources such as path parents.
    OptionalHandle(Option<SafeNativeHandle>),
    /// Terlan list carrying bridge-facing values.
    List(Vec<SafeNativeBridgeValue>),
}

/// Stable dispatcher error returned before crossing a runtime boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchError {
    code: &'static str,
    message: String,
    offset: usize,
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
    /// - A `DispatchError` suitable for the SafeNative boundary.
    ///
    /// Transformation:
    /// - Stores adapter-independent error metadata without exposing backend
    ///   exception types.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
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

/// Dispatches one compiler-native operation to a SafeNative adapter function.
///
/// Inputs:
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: neutral runtime values decoded by the native bridge.
///
/// Output:
/// - `Ok(SafeNativeValue)` with the adapter result.
/// - `Err(DispatchError)` for unknown operation ids, arity mismatches, type
///   mismatches, or adapter-specific stable errors.
///
/// Transformation:
/// - Validates the operation id and argument shapes, calls the corresponding
///   Rust adapter, and converts adapter-specific errors into one dispatch
///   error shape.
pub fn dispatch(
    operation: &str,
    args: &[SafeNativeValue],
) -> Result<SafeNativeValue, DispatchError> {
    validate_arity(operation, args)?;
    match operation {
        "std.data.json.null" => Ok(SafeNativeValue::Json(json::null())),
        "std.data.json.bool" => {
            let value = expect_bool(operation, args, 0)?;
            Ok(SafeNativeValue::Json(json::r#bool(value)))
        }
        "std.data.json.int" => {
            let value = expect_int(operation, args, 0)?;
            Ok(SafeNativeValue::Json(json::int(value)))
        }
        "std.data.json.float" => {
            let value = expect_float(operation, args, 0)?;
            json::float(value)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_json_error)
        }
        "std.data.json.string" => {
            let value = expect_text(operation, args, 0)?;
            Ok(SafeNativeValue::Json(json::string(value)))
        }
        "std.data.json.array" => Ok(SafeNativeValue::Json(json::array())),
        "std.data.json.object" => Ok(SafeNativeValue::Json(json::object())),
        "std.data.json.array_push" | "std.data.json.object_put" => Err(DispatchError::new(
            "dispatch.mutable_receiver_requires_direct_lowering",
            format!(
                "operation `{operation}` mutates a receiver and must use direct native lowering"
            ),
            0,
        )),
        "std.data.json.parse" => {
            let text = expect_text(operation, args, 0)?;
            json::parse(text)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_json_error)
        }
        "std.data.json.stringify" => {
            let value = expect_json(operation, args, 0)?;
            json::stringify(value)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_json_error)
        }
        "std.data.json.get" => {
            let value = expect_json(operation, args, 0)?;
            let key = expect_text(operation, args, 1)?;
            json::get(value, key)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_json_error)
        }
        "std.data.json.length" => {
            let value = expect_json(operation, args, 0)?;
            json::length(value)
                .map(SafeNativeValue::Int)
                .map_err(dispatch_json_error)
        }
        "std.data.json.at" => {
            let value = expect_json(operation, args, 0)?;
            let index = expect_int(operation, args, 1)?;
            json::at(value, index)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_string" => {
            let value = expect_json(operation, args, 0)?;
            json::as_string(value)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_int" => {
            let value = expect_json(operation, args, 0)?;
            json::as_int(value)
                .map(SafeNativeValue::Int)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_float" => {
            let value = expect_json(operation, args, 0)?;
            json::as_float(value)
                .map(SafeNativeValue::Float)
                .map_err(dispatch_json_error)
        }
        "std.data.json.as_bool" => {
            let value = expect_json(operation, args, 0)?;
            json::as_bool(value)
                .map(SafeNativeValue::Bool)
                .map_err(dispatch_json_error)
        }
        "std.data.json.is_null" => {
            let value = expect_json(operation, args, 0)?;
            Ok(SafeNativeValue::Bool(json::is_null(value)))
        }
        "std.http.request.body_json" => {
            let request = expect_http_request(operation, args, 0)?;
            http::body_json(request)
                .map(SafeNativeValue::Json)
                .map_err(dispatch_http_error)
        }
        "std.http.request.body_text" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(SafeNativeValue::Text(http::body_text(request)))
        }
        "std.http.request.method" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(SafeNativeValue::Text(http::method(request)))
        }
        "std.http.request.path" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(SafeNativeValue::Text(http::path(request)))
        }
        "std.http.request.param" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(SafeNativeValue::OptionalText(http::param(request, name)))
        }
        "std.http.request.query" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(SafeNativeValue::OptionalText(http::query(request, name)))
        }
        "std.http.request.query_string" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(SafeNativeValue::Text(http::query_string(request)))
        }
        "std.http.request.header" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(SafeNativeValue::OptionalText(http::request_header(
                request, name,
            )))
        }
        "std.http.request.cookie" => {
            let request = expect_http_request(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(SafeNativeValue::OptionalText(http::cookie(request, name)))
        }
        "std.http.request.cookies" => {
            let request = expect_http_request(operation, args, 0)?;
            Ok(SafeNativeValue::HttpCookieJar(http::cookies(request)))
        }
        "std.http.cookies.get" => {
            let jar = expect_http_cookie_jar(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            Ok(SafeNativeValue::OptionalText(jar.get(name)))
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
                .map(SafeNativeValue::Text)
                .map_err(dispatch_http_error)
        }
        "std.http.cookies.set_header_with_options" => {
            let name = expect_text(operation, args, 0)?;
            let value = expect_text(operation, args, 1)?;
            let options = cookie_options_from_args(operation, args)?;
            http::set_header_with_options(name, value, &options)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_http_error)
        }
        "std.http.cookies.delete_header" => {
            let name = expect_text(operation, args, 0)?;
            let path = expect_text(operation, args, 1)?;
            http::delete_header(name, path)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_http_error)
        }
        "std.http.response.json" => {
            let value = expect_json(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(SafeNativeValue::HttpResponse(http::json(value, status)))
        }
        "std.http.response.json_text" => {
            let value = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(SafeNativeValue::HttpResponse(http::json_text(value, status)))
        }
        "std.http.response.text" => {
            let value = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(SafeNativeValue::HttpResponse(http::text(value, status)))
        }
        "std.http.response.html" => {
            let value = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(SafeNativeValue::HttpResponse(http::html(value, status)))
        }
        "std.http.response.file" => {
            let path = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            let content_type = expect_text(operation, args, 2)?;
            Ok(SafeNativeValue::HttpResponse(http::file(
                path,
                status,
                content_type,
            )))
        }
        "std.http.response.redirect" => {
            let location = expect_text(operation, args, 0)?;
            let status = expect_int(operation, args, 1)?;
            Ok(SafeNativeValue::HttpResponse(http::redirect(
                location, status,
            )))
        }
        "std.http.response.status"
        | "std.http.response.header"
        | "std.http.response.set_cookie_header" => Err(DispatchError::new(
            "dispatch.mutable_receiver_requires_direct_lowering",
            format!(
                "operation `{operation}` mutates a receiver and must use direct native lowering"
            ),
            0,
        )),
        "std.encoding.base64.encode" => {
            let text = expect_text(operation, args, 0)?;
            Ok(SafeNativeValue::Text(base64::encode(text)))
        }
        "std.encoding.base64.decode" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode(text)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_base64_error)
        }
        "std.encoding.base64.encode_url" => {
            let text = expect_text(operation, args, 0)?;
            Ok(SafeNativeValue::Text(base64::encode_url(text)))
        }
        "std.encoding.base64.decode_url" => {
            let text = expect_text(operation, args, 0)?;
            base64::decode_url(text)
                .map(SafeNativeValue::Text)
                .map_err(dispatch_base64_error)
        }
        "std.io.path.from_string" => {
            let text = expect_text(operation, args, 0)?;
            path::from_string(text)
                .map(SafeNativeValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.to_string" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::Text(path::to_string(value)))
        }
        "std.io.path.join" => {
            let value = expect_path(operation, args, 0)?;
            let child = expect_text(operation, args, 1)?;
            path::join(value, child)
                .map(SafeNativeValue::Path)
                .map_err(dispatch_path_error)
        }
        "std.io.path.file_name" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(path::file_name(value)))
        }
        "std.io.path.extension" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(path::extension(value)))
        }
        "std.io.path.parent" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalPath(path::parent(value)))
        }
        "std.io.path.is_absolute" => {
            let value = expect_path(operation, args, 0)?;
            Ok(SafeNativeValue::Bool(path::is_absolute(value)))
        }
        "std.net.uri.parse" => {
            let text = expect_text(operation, args, 0)?;
            uri::parse(text)
                .map(SafeNativeValue::Uri)
                .map_err(dispatch_uri_error)
        }
        "std.net.uri.to_string" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::Text(uri::to_string(value)))
        }
        "std.net.uri.scheme" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::Text(uri::scheme(value)))
        }
        "std.net.uri.host" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(uri::host(value)))
        }
        "std.net.uri.path" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::Text(uri::path(value)))
        }
        "std.net.uri.query" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(uri::query(value)))
        }
        "std.net.uri.fragment" => {
            let value = expect_uri(operation, args, 0)?;
            Ok(SafeNativeValue::OptionalText(uri::fragment(value)))
        }
        "std.db.postgres.connect" => {
            let config = expect_postgres_config(operation, args, 0)?;
            postgres::connect(config)
                .map(SafeNativeValue::PostgresPool)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.query" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::query(pool, sql, params)
                .map(SafeNativeValue::PostgresRows)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.query_one" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::query_one(pool, sql, params)
                .map(SafeNativeValue::OptionalPostgresRow)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.execute" => {
            let pool = expect_postgres_pool(operation, args, 0)?;
            let sql = expect_text(operation, args, 1)?;
            let params = expect_json_list(operation, args, 2)?;
            postgres::execute(pool, sql, params)
                .map(SafeNativeValue::Int)
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
                .map(SafeNativeValue::Text)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.int" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::int(row, name)
                .map(SafeNativeValue::Int)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.bool" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::r#bool(row, name)
                .map(SafeNativeValue::Bool)
                .map_err(dispatch_postgres_error)
        }
        "std.db.postgres.json" => {
            let row = expect_postgres_row(operation, args, 0)?;
            let name = expect_text(operation, args, 1)?;
            postgres::json(row, name)
                .map(SafeNativeValue::Json)
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
fn validate_arity(operation: &str, args: &[SafeNativeValue]) -> Result<(), DispatchError> {
    validate_operation_arity(operation, args.len(), unknown_operation)
}

#[cfg(test)]
#[path = "dispatch_test.rs"]
mod dispatch_test;
