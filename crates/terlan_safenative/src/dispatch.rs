//! Pure operation dispatcher for Rust-backed SafeNative std adapters.
//!
//! This module is the first shared execution surface between compiler-native
//! operation ids such as `std.data.json.parse` and concrete Rust adapter
//! functions. The BEAM/native worker layer can call this module after it has
//! decoded runtime terms into `SafeNativeValue`.

use crate::handle::SafeNativeHandle;
use crate::resource::{ResourceStore, ResourceValue};
use crate::{base64, http, json, path, postgres, uri, vector};

mod args;
mod arity;

use args::{
    cookie_options_from_args, dispatch_base64_error, dispatch_http_error, dispatch_json_error,
    dispatch_path_error, dispatch_postgres_error, dispatch_resource_error, dispatch_uri_error,
    dispatch_vector_error, expect_bool, expect_bridge_bool, expect_bridge_handle,
    expect_bridge_int, expect_bridge_list, expect_bridge_text, expect_float,
    expect_http_cookie_jar, expect_http_request, expect_int, expect_json, expect_json_list,
    expect_path, expect_postgres_config, expect_postgres_pool, expect_postgres_row, expect_text,
    expect_uri, type_error, unknown_operation,
};
pub use arity::operation_arity;

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

/// Dispatches an operation through handle-backed resource ownership.
///
/// Inputs:
/// - `store`: resource store owned by the native worker.
/// - `operation`: compiler-native operation id from `@compiler.native`.
/// - `args`: bridge-facing values where opaque adapter values are handles.
///
/// Output:
/// - `Ok(SafeNativeBridgeValue)` with opaque adapter outputs stored and
///   returned as handles.
/// - `Err(DispatchError)` for unknown operations, arity/type mismatches,
///   stale handles, resource kind mismatches, or adapter failures.
///
/// Transformation:
/// - Validates operation arity, decodes bridge handles into pure adapter
///   values, calls `dispatch`, and stores opaque adapter outputs back into the
///   resource store before returning handles.
pub fn dispatch_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    validate_bridge_arity(operation, args)?;
    if operation == "std.http.cookies.set" {
        return dispatch_cookie_set_with_resources(store, operation, args);
    }
    if operation == "std.http.cookies.delete" {
        return dispatch_cookie_delete_with_resources(store, operation, args);
    }
    if operation.starts_with("std.native.collections.vector.") {
        return dispatch_native_vector_with_resources(store, operation, args);
    }
    let decoded = decode_bridge_args(store, operation, args)?;
    let result = dispatch(operation, &decoded)?;
    encode_bridge_result(store, result)
}

/// Dispatches a native vector operation through resource ownership.
///
/// Inputs:
/// - `store`: resource registry owning vector handles.
/// - `operation`: compiler-native vector operation id.
/// - `args`: bridge-facing vector arguments.
///
/// Output:
/// - Bridge value result for the vector operation.
/// - `DispatchError` for bad arity, bad handle, bad argument, or vector
///   bounds failures.
///
/// Transformation:
/// - Allocates, reads, or mutates Rust-owned vector resources while preserving
///   stable opaque handles for BEAM-side code.
fn dispatch_native_vector_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    match operation {
        "std.native.collections.vector.new" => store
            .insert(ResourceValue::NativeVector(vector::new()))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        "std.native.collections.vector.from_list" => {
            let values = expect_bridge_list(operation, args, 0)?;
            store
                .insert(ResourceValue::NativeVector(vector::from_list(
                    values.to_vec(),
                )))
                .map(SafeNativeBridgeValue::Handle)
                .map_err(dispatch_resource_error)
        }
        "std.native.collections.vector.length" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            vector::length(
                store
                    .native_vector(handle)
                    .map_err(dispatch_resource_error)?,
            )
            .map(SafeNativeBridgeValue::Int)
            .map_err(dispatch_vector_error)
        }
        "std.native.collections.vector.get_at" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let index = expect_bridge_int(operation, args, 1)?;
            vector::get_at(
                store
                    .native_vector(handle)
                    .map_err(dispatch_resource_error)?,
                index,
            )
            .map_err(dispatch_vector_error)
        }
        "std.native.collections.vector.set_at" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let index = expect_bridge_int(operation, args, 1)?;
            let value = args
                .get(2)
                .cloned()
                .ok_or_else(|| type_error(operation, 2, "value"))?;
            vector::set_at(
                store
                    .native_vector_mut(handle)
                    .map_err(dispatch_resource_error)?,
                index,
                value,
            )
            .map_err(dispatch_vector_error)?;
            Ok(SafeNativeBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.swap" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let left = expect_bridge_int(operation, args, 1)?;
            let right = expect_bridge_int(operation, args, 2)?;
            vector::swap(
                store
                    .native_vector_mut(handle)
                    .map_err(dispatch_resource_error)?,
                left,
                right,
            )
            .map_err(dispatch_vector_error)?;
            Ok(SafeNativeBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.push" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            let value = args
                .get(1)
                .cloned()
                .ok_or_else(|| type_error(operation, 1, "value"))?;
            vector::push(
                store
                    .native_vector_mut(handle)
                    .map_err(dispatch_resource_error)?,
                value,
            );
            Ok(SafeNativeBridgeValue::Handle(handle))
        }
        "std.native.collections.vector.to_list" => {
            let handle = expect_bridge_handle(operation, args, 0)?;
            store
                .native_vector(handle)
                .map(|vector| SafeNativeBridgeValue::List(vector::to_list(vector)))
                .map_err(dispatch_resource_error)
        }
        _ => Err(unknown_operation(operation)),
    }
}

/// Mutates a cookie jar resource through `std.http.cookies.set`.
///
/// Inputs:
/// - `store`: resource registry owning the cookie jar.
/// - `operation`: compiler-native operation id used in diagnostics.
/// - `args`: bridge arguments containing jar handle and cookie values.
///
/// Output:
/// - `Unit` when the cookie mutation is recorded.
/// - `DispatchError` for bad handle, argument, or cookie validation failures.
///
/// Transformation:
/// - Borrows the jar mutably from the resource store and appends one
///   `Set-Cookie` mutation without cloning the jar.
fn dispatch_cookie_set_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    let handle = expect_bridge_handle(operation, args, 0)?;
    let name = expect_bridge_text(operation, args, 1)?;
    let value = expect_bridge_text(operation, args, 2)?;
    let path = expect_bridge_text(operation, args, 3)?;
    let http_only = expect_bridge_bool(operation, args, 4)?;
    let secure = expect_bridge_bool(operation, args, 5)?;
    store
        .http_cookie_jar_mut(handle)
        .map_err(dispatch_resource_error)?
        .set(name, value, path, http_only, secure)
        .map_err(dispatch_http_error)?;
    Ok(SafeNativeBridgeValue::Unit)
}

/// Mutates a cookie jar resource through `std.http.cookies.delete`.
///
/// Inputs:
/// - `store`: resource registry owning the cookie jar.
/// - `operation`: compiler-native operation id used in diagnostics.
/// - `args`: bridge arguments containing jar handle, cookie name, and path.
///
/// Output:
/// - `Unit` when the deletion mutation is recorded.
/// - `DispatchError` for bad handle, argument, or cookie validation failures.
///
/// Transformation:
/// - Borrows the jar mutably from the resource store and appends one expiring
///   `Set-Cookie` mutation without cloning the jar.
fn dispatch_cookie_delete_with_resources(
    store: &mut ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<SafeNativeBridgeValue, DispatchError> {
    let handle = expect_bridge_handle(operation, args, 0)?;
    let name = expect_bridge_text(operation, args, 1)?;
    let path = expect_bridge_text(operation, args, 2)?;
    store
        .http_cookie_jar_mut(handle)
        .map_err(dispatch_resource_error)?
        .delete(name, path)
        .map_err(dispatch_http_error)?;
    Ok(SafeNativeBridgeValue::Unit)
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
    match operation_arity(operation) {
        Some(expected) if expected == args.len() => Ok(()),
        Some(expected) => Err(DispatchError::new(
            "dispatch.arity",
            format!(
                "Operation `{operation}` expects {expected} argument(s), got {}.",
                args.len()
            ),
            0,
        )),
        None => Err(unknown_operation(operation)),
    }
}

/// Validates bridge argument count for one operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `args`: bridge-facing values supplied by the worker boundary.
///
/// Output:
/// - `Ok(())` when arity matches.
/// - `Err(DispatchError)` for unknown operations or wrong arity.
///
/// Transformation:
/// - Compares supplied bridge argument count with `operation_arity`.
fn validate_bridge_arity(
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<(), DispatchError> {
    match operation_arity(operation) {
        Some(expected) if expected == args.len() => Ok(()),
        Some(expected) => Err(DispatchError::new(
            "dispatch.arity",
            format!(
                "Operation `{operation}` expects {expected} argument(s), got {}.",
                args.len()
            ),
            0,
        )),
        None => Err(unknown_operation(operation)),
    }
}

/// Decodes bridge-facing arguments into pure dispatch values.
///
/// Inputs:
/// - `store`: resource store used to resolve opaque handles.
/// - `operation`: compiler-native operation id.
/// - `args`: bridge-facing operation arguments.
///
/// Output:
/// - Pure dispatch values suitable for `dispatch`.
/// - `Err(DispatchError)` when a handle is stale or has the wrong kind.
///
/// Transformation:
/// - Resolves handles according to the operation family and clones the
///   adapter-owned value for pure dispatch.
fn decode_bridge_args(
    store: &ResourceStore,
    operation: &str,
    args: &[SafeNativeBridgeValue],
) -> Result<Vec<SafeNativeValue>, DispatchError> {
    args.iter()
        .enumerate()
        .map(|(index, arg)| decode_bridge_arg(store, operation, index, arg))
        .collect()
}

/// Decodes one bridge-facing argument into a pure dispatch value.
///
/// Inputs:
/// - `store`: resource store used to resolve opaque handles.
/// - `operation`: compiler-native operation id.
/// - `index`: argument index for diagnostics.
/// - `arg`: bridge-facing argument.
///
/// Output:
/// - Pure dispatch value.
/// - `Err(DispatchError)` for unsupported bridge value shapes.
///
/// Transformation:
/// - Converts primitive bridge values directly and resolves handles to the
///   resource kind implied by the operation namespace.
fn decode_bridge_arg(
    store: &ResourceStore,
    operation: &str,
    index: usize,
    arg: &SafeNativeBridgeValue,
) -> Result<SafeNativeValue, DispatchError> {
    match arg {
        SafeNativeBridgeValue::Unit => Ok(SafeNativeValue::Unit),
        SafeNativeBridgeValue::Text(value) => Ok(SafeNativeValue::Text(value.clone())),
        SafeNativeBridgeValue::Int(value) => Ok(SafeNativeValue::Int(*value)),
        SafeNativeBridgeValue::Float(value) => Ok(SafeNativeValue::Float(*value)),
        SafeNativeBridgeValue::Bool(value) => Ok(SafeNativeValue::Bool(*value)),
        SafeNativeBridgeValue::PostgresConfig(value) => {
            Ok(SafeNativeValue::PostgresConfig(value.clone()))
        }
        SafeNativeBridgeValue::Handle(handle) if operation == "std.http.response.json" => store
            .json(*handle)
            .cloned()
            .map(SafeNativeValue::Json)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.data.json.") => store
            .json(*handle)
            .cloned()
            .map(SafeNativeValue::Json)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.http.request.") => {
            store
                .http_request(*handle)
                .cloned()
                .map(SafeNativeValue::HttpRequest)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.http.cookies.") => {
            store
                .http_cookie_jar(*handle)
                .cloned()
                .map(SafeNativeValue::HttpCookieJar)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.http.response.") => {
            store
                .http_response(*handle)
                .cloned()
                .map(SafeNativeValue::HttpResponse)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.io.path.") => store
            .path(*handle)
            .cloned()
            .map(SafeNativeValue::Path)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle) if operation.starts_with("std.net.uri.") => store
            .uri(*handle)
            .cloned()
            .map(SafeNativeValue::Uri)
            .map_err(dispatch_resource_error),
        SafeNativeBridgeValue::Handle(handle)
            if matches!(
                operation,
                "std.db.postgres.query"
                    | "std.db.postgres.query_one"
                    | "std.db.postgres.execute"
                    | "std.db.postgres.transaction"
            ) && index == 0 =>
        {
            store
                .postgres_pool(*handle)
                .cloned()
                .map(SafeNativeValue::PostgresPool)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(handle)
            if matches!(
                operation,
                "std.db.postgres.string"
                    | "std.db.postgres.int"
                    | "std.db.postgres.bool"
                    | "std.db.postgres.json"
            ) && index == 0 =>
        {
            store
                .postgres_row(*handle)
                .cloned()
                .map(SafeNativeValue::PostgresRow)
                .map_err(dispatch_resource_error)
        }
        SafeNativeBridgeValue::Handle(_) => Err(type_error(operation, index, "non-handle value")),
        SafeNativeBridgeValue::OptionalText(_) | SafeNativeBridgeValue::OptionalHandle(_) => {
            Err(type_error(operation, index, "non-optional argument"))
        }
        SafeNativeBridgeValue::List(values) => Ok(SafeNativeValue::JsonList(
            values
                .iter()
                .enumerate()
                .map(|(list_index, value)| match value {
                    SafeNativeBridgeValue::Handle(handle) => store
                        .json(*handle)
                        .cloned()
                        .map_err(dispatch_resource_error),
                    _ => Err(type_error(operation, list_index, "Json handle")),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
    }
}

/// Encodes a pure dispatch result into a bridge-facing value.
///
/// Inputs:
/// - `store`: resource store that will own opaque adapter outputs.
/// - `value`: pure dispatch result.
///
/// Output:
/// - Bridge-facing result with opaque values represented as handles.
/// - `Err(DispatchError)` when resource insertion fails.
///
/// Transformation:
/// - Stores JSON/path/URI outputs in the resource store and returns only their
///   handles across the bridge surface.
fn encode_bridge_result(
    store: &mut ResourceStore,
    value: SafeNativeValue,
) -> Result<SafeNativeBridgeValue, DispatchError> {
    match value {
        SafeNativeValue::Unit => Ok(SafeNativeBridgeValue::Unit),
        SafeNativeValue::Text(value) => Ok(SafeNativeBridgeValue::Text(value)),
        SafeNativeValue::Int(value) => Ok(SafeNativeBridgeValue::Int(value)),
        SafeNativeValue::Float(value) => Ok(SafeNativeBridgeValue::Float(value)),
        SafeNativeValue::Bool(value) => Ok(SafeNativeBridgeValue::Bool(value)),
        SafeNativeValue::Json(value) => store
            .insert(ResourceValue::Json(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::HttpRequest(value) => store
            .insert(ResourceValue::HttpRequest(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::HttpResponse(value) => store
            .insert(ResourceValue::HttpResponse(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::HttpCookieJar(value) => store
            .insert(ResourceValue::HttpCookieJar(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::Path(value) => store
            .insert(ResourceValue::Path(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::Uri(value) => store
            .insert(ResourceValue::Uri(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresPool(value) => store
            .insert(ResourceValue::PostgresPool(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresRow(value) => store
            .insert(ResourceValue::PostgresRow(value))
            .map(SafeNativeBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresRows(values) => values
            .into_iter()
            .map(|row| {
                store
                    .insert(ResourceValue::PostgresRow(row))
                    .map(SafeNativeBridgeValue::Handle)
                    .map_err(dispatch_resource_error)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(SafeNativeBridgeValue::List),
        SafeNativeValue::OptionalPostgresRow(value) => value
            .map(|row| store.insert(ResourceValue::PostgresRow(row)))
            .transpose()
            .map(SafeNativeBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
        SafeNativeValue::PostgresConfig(_) | SafeNativeValue::JsonList(_) => {
            Err(DispatchError::new(
                "dispatch.postgres_requires_runtime_bridge",
                "Postgres input-only values cannot be returned across the runtime bridge.",
                0,
            ))
        }
        SafeNativeValue::OptionalText(value) => Ok(SafeNativeBridgeValue::OptionalText(value)),
        SafeNativeValue::OptionalPath(value) => value
            .map(|path| store.insert(ResourceValue::Path(path)))
            .transpose()
            .map(SafeNativeBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
    }
}

#[cfg(test)]
#[path = "dispatch_test.rs"]
mod dispatch_test;
