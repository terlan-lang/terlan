//! Operation arity table for NativeBoundary dispatch.
//!
//! The dispatcher owns execution, while this module owns the compact arity
//! contract used before execution and by manifest coverage tests.

use super::DispatchError;

/// Rust-backed operation arities accepted by NativeBoundary dispatch.
///
/// Inputs:
/// - Static operation ids from `@compiler.native` declarations.
///
/// Output:
/// - Operation id to runtime argument count mapping.
///
/// Transformation:
/// - Keeps arity validation data compact and aligned with
///   `std/RUST_BACKED_MANIFEST.tsv` while leaving operation execution in the
///   explicit dispatch match.
const OPERATION_ARITIES: &[(&str, usize)] = &[
    ("std.data.json.array", 0),
    ("std.data.json.array_push", 2),
    ("std.data.json.as_bool", 1),
    ("std.data.json.as_float", 1),
    ("std.data.json.as_int", 1),
    ("std.data.json.as_string", 1),
    ("std.data.json.at", 2),
    ("std.data.json.bool", 1),
    ("std.data.json.float", 1),
    ("std.data.json.get", 2),
    ("std.data.json.int", 1),
    ("std.data.json.is_null", 1),
    ("std.data.json.length", 1),
    ("std.data.json.null", 0),
    ("std.data.json.object", 0),
    ("std.data.json.object_put", 3),
    ("std.data.json.parse", 1),
    ("std.data.json.string", 1),
    ("std.data.json.stringify", 1),
    ("std.db.postgres.bool", 2),
    ("std.db.postgres.connect", 1),
    ("std.db.postgres.execute", 3),
    ("std.db.postgres.int", 2),
    ("std.db.postgres.json", 2),
    ("std.db.postgres.query", 3),
    ("std.db.postgres.query_one", 3),
    ("std.db.postgres.string", 2),
    ("std.db.postgres.transaction", 2),
    ("std.encoding.base64.decode", 1),
    ("std.encoding.base64.decode_bytes", 1),
    ("std.encoding.base64.decode_url", 1),
    ("std.encoding.base64.decode_url_bytes", 1),
    ("std.encoding.base64.encode", 1),
    ("std.encoding.base64.encode_bytes", 1),
    ("std.encoding.base64.encode_url", 1),
    ("std.encoding.base64.encode_url_bytes", 1),
    ("std.encoding.md5.digest", 1),
    ("std.http.request.body_json", 1),
    ("std.http.request.body_text", 1),
    ("std.http.request.cookie", 2),
    ("std.http.request.cookies", 1),
    ("std.http.request.header", 2),
    ("std.http.request.method", 1),
    ("std.http.request.param", 2),
    ("std.http.request.path", 1),
    ("std.http.request.query", 2),
    ("std.http.request.query_string", 1),
    ("std.http.cookies.delete", 3),
    ("std.http.cookies.delete_header", 2),
    ("std.http.cookies.get", 2),
    ("std.http.cookies.set", 6),
    ("std.http.cookies.set_header", 5),
    ("std.http.cookies.set_header_with_options", 10),
    ("std.http.response.header", 3),
    ("std.http.response.file", 3),
    ("std.http.response.html", 2),
    ("std.http.response.json", 2),
    ("std.http.response.json_text", 2),
    ("std.http.response.redirect", 2),
    ("std.http.response.stream", 5),
    ("std.http.response.set_cookie_header", 2),
    ("std.http.response.status", 2),
    ("std.http.response.text", 2),
    ("std.http.response.with_cookies", 2),
    ("std.http.sse.data", 1),
    ("std.http.sse.endpoint", 2),
    ("std.http.sse.endpoint_with_keep_alive", 3),
    ("std.http.sse.response", 2),
    ("std.http.sse.with_id", 2),
    ("std.http.sse.with_name", 2),
    ("std.http.sse.with_retry_ms", 2),
    ("std.http.websocket.close", 0),
    ("std.http.websocket.endpoint", 2),
    ("std.http.websocket.ping", 1),
    ("std.http.websocket.pong", 1),
    ("std.http.websocket.text", 1),
    ("std.http.session.current", 1),
    ("std.http.session.get", 2),
    ("std.http.session.set", 3),
    ("std.http.session.delete", 2),
    ("std.http.session.rotate", 1),
    ("std.http.session.expire", 1),
    ("std.http.session.with_response", 2),
    ("std.io.path.extension", 1),
    ("std.io.path.file_name", 1),
    ("std.io.path.from_string", 1),
    ("std.io.path.is_absolute", 1),
    ("std.io.path.join", 2),
    ("std.io.path.parent", 1),
    ("std.io.path.to_string", 1),
    ("std.io.console.println", 1),
    ("std.io.file.exists", 1),
    ("std.io.file.read_text", 1),
    ("std.io.file.write_text", 2),
    ("std.io.file.append_text", 2),
    ("std.io.file.delete", 1),
    ("std.net.uri.fragment", 1),
    ("std.net.uri.host", 1),
    ("std.net.uri.parse", 1),
    ("std.net.uri.path", 1),
    ("std.net.uri.query", 1),
    ("std.net.uri.scheme", 1),
    ("std.net.uri.to_string", 1),
    ("std.random.random.bool", 1),
    ("std.random.random.bounded_int", 3),
    ("std.random.random.choice", 2),
    ("std.random.random.entropy", 0),
    ("std.random.random.float", 1),
    ("std.random.random.int", 1),
    ("std.random.random.sample", 3),
    ("std.random.random.seed", 1),
    ("std.random.random.shuffle", 2),
    ("std.regex.regex.capture", 3),
    ("std.regex.regex.compile", 1),
    ("std.regex.regex.escape", 1),
    ("std.regex.regex.find", 2),
    ("std.regex.regex.find_all", 2),
    ("std.regex.regex.is_match", 2),
    ("std.regex.regex.named_capture", 3),
    ("std.regex.regex.replace", 3),
    ("std.regex.regex.split", 2),
    ("std.native.collections.vector.new", 0),
    ("std.native.collections.vector.from_list", 1),
    ("std.native.collections.vector.length", 1),
    ("std.native.collections.vector.get", 2),
    ("std.native.collections.vector.get_at", 2),
    ("std.native.collections.vector.set_at", 3),
    ("std.native.collections.vector.swap", 3),
    ("std.native.collections.vector.push", 2),
    ("std.native.collections.vector.to_list", 1),
];

/// Returns the expected arity for a supported operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
///
/// Output:
/// - Expected runtime argument count, or `None` for an unknown operation.
///
/// Transformation:
/// - Looks up operation ids in `OPERATION_ARITIES` without allocating, giving
///   both pure dispatch and bridge dispatch one shared arity source.
pub fn operation_arity(operation: &str) -> Option<usize> {
    OPERATION_ARITIES
        .iter()
        .find_map(|(candidate, arity)| (*candidate == operation).then_some(*arity))
}

/// Validates the supplied argument count for one operation.
///
/// Inputs:
/// - `operation`: compiler-native operation id.
/// - `actual`: supplied argument count.
/// - `unknown`: operation-specific unknown-operation diagnostic builder.
///
/// Output:
/// - `Ok(())` when arity matches.
/// - `Err(DispatchError)` for unknown operations or wrong arity.
///
/// Transformation:
/// - Compares the supplied count with `operation_arity` while allowing each
///   dispatch surface to keep its own unknown-operation error context.
pub fn validate_operation_arity(
    operation: &str,
    actual: usize,
    unknown: impl FnOnce(&str) -> DispatchError,
) -> Result<(), DispatchError> {
    match operation_arity(operation) {
        Some(expected) if expected == actual => Ok(()),
        Some(expected) => Err(DispatchError::new(
            "dispatch.arity",
            format!("Operation `{operation}` expects {expected} argument(s), got {actual}."),
            0,
        )),
        None => Err(unknown(operation)),
    }
}
