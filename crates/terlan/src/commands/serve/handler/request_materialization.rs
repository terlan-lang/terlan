//! Projection-aware materialization of the fixed managed HTTP Request envelope.

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::vm::ReplValue;

/// Builds the exact Request layout while omitting payloads that typed NativeIR
/// proves are unobservable to the selected direct export.
pub(in crate::commands::serve) fn vm_request_descriptor_owned(
    request: RequestParts,
    projection: RequestFieldProjection,
) -> ReplValue {
    let cookies = if projection.requires(RequestFieldProjection::COOKIES)
        || projection.requires(RequestFieldProjection::COOKIE_JAR)
    {
        owned_string_map(request.cookies)
    } else {
        empty_string_map()
    };
    let direct_cookies = if projection.requires(RequestFieldProjection::COOKIES) {
        cookies.clone()
    } else {
        empty_string_map()
    };
    let jar_cookies = if projection.requires(RequestFieldProjection::COOKIE_JAR) {
        cookies
    } else {
        empty_string_map()
    };
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        projected_string(projection, RequestFieldProjection::METHOD, request.method),
        projected_string(projection, RequestFieldProjection::PATH, request.path),
        projected_map(projection, RequestFieldProjection::PARAMS, request.params),
        projected_string(projection, RequestFieldProjection::BODY, request.body),
        projected_string(
            projection,
            RequestFieldProjection::QUERY_STRING,
            request.query_string,
        ),
        projected_map(projection, RequestFieldProjection::QUERY, request.query),
        projected_map(projection, RequestFieldProjection::HEADERS, request.headers),
        direct_cookies,
        ReplValue::Tuple(vec![jar_cookies, ReplValue::List(Vec::new())]),
        projected_string(
            projection,
            RequestFieldProjection::BODY_FILE_PATH,
            request.body_file_path,
        ),
    ])
}

/// Builds the source-visible request tuple used by pattern-head route handlers.
pub(in crate::commands::serve) fn vm_source_request_tuple_owned(
    request: RequestParts,
) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("request".to_string()),
        ReplValue::String(request.method),
        ReplValue::String(request.path),
        owned_string_map(request.params),
        ReplValue::String(request.body),
        ReplValue::String(request.query_string),
        owned_string_map(request.query),
        owned_string_map(request.headers),
        owned_string_map(request.cookies),
        ReplValue::String(request.body_file_path),
    ])
}

/// Replaces one fixed Request envelope while retaining its aggregate vectors.
pub(in crate::commands::serve) fn replace_vm_request_descriptor(
    value: &mut ReplValue,
    request: RequestParts,
    projection: RequestFieldProjection,
) {
    let ReplValue::Tuple(fields) = value else {
        *value = vm_request_descriptor_owned(request, projection);
        return;
    };
    if fields.len() != 11 {
        *value = vm_request_descriptor_owned(request, projection);
        return;
    }
    fields[0] = ReplValue::Int(0);
    fields[1] = projected_string(projection, RequestFieldProjection::METHOD, request.method);
    fields[2] = projected_string(projection, RequestFieldProjection::PATH, request.path);
    replace_projected_map(
        &mut fields[3],
        projection,
        RequestFieldProjection::PARAMS,
        request.params,
    );
    fields[4] = projected_string(projection, RequestFieldProjection::BODY, request.body);
    fields[5] = projected_string(
        projection,
        RequestFieldProjection::QUERY_STRING,
        request.query_string,
    );
    replace_projected_map(
        &mut fields[6],
        projection,
        RequestFieldProjection::QUERY,
        request.query,
    );
    replace_projected_map(
        &mut fields[7],
        projection,
        RequestFieldProjection::HEADERS,
        request.headers,
    );
    let direct_cookies = projection.requires(RequestFieldProjection::COOKIES);
    let jar_cookies = projection.requires(RequestFieldProjection::COOKIE_JAR);
    let (direct_entries, jar_entries) = match (direct_cookies, jar_cookies) {
        (true, true) => (request.cookies.clone(), request.cookies),
        (true, false) => (request.cookies, Vec::new()),
        (false, true) => (Vec::new(), request.cookies),
        (false, false) => (Vec::new(), Vec::new()),
    };
    replace_string_map(&mut fields[8], direct_entries);
    fields[10] = projected_string(
        projection,
        RequestFieldProjection::BODY_FILE_PATH,
        request.body_file_path,
    );
    let ReplValue::Tuple(jar) = &mut fields[9] else {
        fields[9] = ReplValue::Tuple(vec![
            ReplValue::Map(Vec::new()),
            ReplValue::List(Vec::new()),
        ]);
        replace_cookie_jar(&mut fields[9], jar_entries);
        return;
    };
    if jar.len() != 2 {
        *jar = vec![ReplValue::Map(Vec::new()), ReplValue::List(Vec::new())];
    }
    replace_string_map(&mut jar[0], jar_entries);
    if let ReplValue::List(pending) = &mut jar[1] {
        pending.clear();
    } else {
        jar[1] = ReplValue::List(Vec::new());
    }
}

fn replace_projected_map(
    value: &mut ReplValue,
    projection: RequestFieldProjection,
    field: usize,
    entries: Vec<(String, String)>,
) {
    replace_string_map(
        value,
        if projection.requires(field) {
            entries
        } else {
            Default::default()
        },
    );
}

fn replace_string_map(value: &mut ReplValue, entries: Vec<(String, String)>) {
    let ReplValue::Map(existing) = value else {
        *value = owned_string_map(entries);
        return;
    };
    existing.clear();
    existing.extend(
        entries
            .into_iter()
            .map(|(key, value)| (ReplValue::String(key), ReplValue::String(value))),
    );
}

fn replace_cookie_jar(value: &mut ReplValue, entries: Vec<(String, String)>) {
    let ReplValue::Tuple(fields) = value else {
        return;
    };
    replace_string_map(&mut fields[0], entries);
}

fn projected_string(projection: RequestFieldProjection, field: usize, value: String) -> ReplValue {
    ReplValue::String(if projection.requires(field) {
        value
    } else {
        String::new()
    })
}

fn projected_map(
    projection: RequestFieldProjection,
    field: usize,
    entries: Vec<(String, String)>,
) -> ReplValue {
    if projection.requires(field) {
        owned_string_map(entries)
    } else {
        empty_string_map()
    }
}

fn owned_string_map(entries: Vec<(String, String)>) -> ReplValue {
    ReplValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (ReplValue::String(key), ReplValue::String(value)))
            .collect(),
    )
}

fn empty_string_map() -> ReplValue {
    ReplValue::Map(Vec::new())
}

#[cfg(test)]
#[path = "request_projection_test.rs"]
#[cfg(test)]
mod request_projection_test;
