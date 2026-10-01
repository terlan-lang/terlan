//! Package-owned materialization of source-visible HTTP request values.

use crate::{RequestFieldProjection as Projection, RequestParts};
use terlan_runtime_abi::{native_record, NativeValue};

/// Materializes the private source Request record. Unobserved fields keep their
/// ordinary empty values. Cookie jars are constructed by Terlan, not ingress.
pub fn request_descriptor(request: RequestParts, projection: Projection) -> NativeValue {
    let RequestParts {
        method,
        path,
        params,
        body,
        query_string,
        query,
        headers,
        cookies,
        body_file_path,
    } = request;
    let method = projected_string(projection, Projection::METHOD, method);
    let path = projected_string(projection, Projection::PATH, path);
    let params = projected_map(projection, Projection::PARAMS, params);
    let body = projected_string(projection, Projection::BODY, body);
    let query_string = projected_string(projection, Projection::QUERY_STRING, query_string);
    let query = projected_map(projection, Projection::QUERY, query);
    let headers = projected_map(projection, Projection::HEADERS, headers);
    let cookies = projected_map(projection, Projection::COOKIES, cookies);
    let body_file_path = projected_string(projection, Projection::BODY_FILE_PATH, body_file_path);
    native_record!(Request, { method, path, params, body, query_string, query, headers, cookies, body_file_path })
}

/// The source-visible request tuple used by pattern-head route handlers.
pub fn source_request_tuple(request: RequestParts) -> NativeValue {
    NativeValue::Tuple(vec![
        NativeValue::Atom("request".into()),
        request.method.into(),
        request.path.into(),
        string_map(request.params),
        request.body.into(),
        request.query_string.into(),
        string_map(request.query),
        string_map(request.headers),
        string_map(request.cookies),
        request.body_file_path.into(),
    ])
}

fn projected_string(projection: Projection, field: usize, value: String) -> String {
    if projection.requires(field) {
        value
    } else {
        String::new()
    }
}

fn projected_map(
    projection: Projection,
    field: usize,
    entries: Vec<(String, String)>,
) -> NativeValue {
    string_map(if projection.requires(field) {
        entries
    } else {
        Vec::new()
    })
}

fn string_map(entries: Vec<(String, String)>) -> NativeValue {
    NativeValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    )
}

#[cfg(test)]
#[path = "request_value_test.rs"]
mod tests;
