//! Direct managed allocation of a compiler-projected HTTP Request.

use smallvec::SmallVec;

use crate::runtime::native::http::{RequestFieldProjection, RequestParts};
use crate::runtime::native_image::managed::{
    ActorHeap, ManagedAggregateDescriptor, ManagedAggregateKind, ManagedFieldType,
    ManagedFieldValue, ManagedLayoutRegistry, SemanticTypeId,
};
use crate::runtime::vm::ReplValue;

use super::managed_values::{allocate_field, AllocationMemo, MAX_PUBLIC_MANAGED_VALUES};

const REQUEST_FIELDS: usize = 10;

/// Allocates the stable opaque Request layout without constructing a generic
/// ten-field `ReplValue` aggregate on the host side first.
pub(super) fn allocate_projected_request(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    request: RequestParts,
    projection: RequestFieldProjection,
) -> Result<i64, String> {
    let descriptor = unique_tuple_layout(layouts, semantic, REQUEST_FIELDS, "Request")?;
    let RequestParts {
        method,
        path,
        body,
        params,
        query_string,
        query,
        headers,
        cookies,
    } = request;
    let direct_cookies = projection.requires(RequestFieldProjection::COOKIES);
    let jar_cookies = projection.requires(RequestFieldProjection::COOKIE_JAR);
    let (direct_cookie_entries, jar_cookie_entries) = match (direct_cookies, jar_cookies) {
        (true, true) => (cookies.clone(), cookies),
        (true, false) => (cookies, Vec::new()),
        (false, true) => (Vec::new(), cookies),
        (false, false) => (Vec::new(), Vec::new()),
    };
    let values = [
        ReplValue::Int(0),
        projected_string(projection, RequestFieldProjection::METHOD, method),
        projected_string(projection, RequestFieldProjection::PATH, path),
        projected_map(projection, RequestFieldProjection::PARAMS, params),
        projected_string(projection, RequestFieldProjection::BODY, body),
        projected_string(
            projection,
            RequestFieldProjection::QUERY_STRING,
            query_string,
        ),
        projected_map(projection, RequestFieldProjection::QUERY, query),
        projected_map(projection, RequestFieldProjection::HEADERS, headers),
        string_map(direct_cookie_entries),
    ];
    let mut budget = MAX_PUBLIC_MANAGED_VALUES;
    let mut memo = AllocationMemo::default();
    let mut allocated = SmallVec::<[ManagedFieldValue; REQUEST_FIELDS]>::new();
    for (field, value) in descriptor.fields().iter().take(9).zip(&values) {
        allocated.push(allocate_field(
            heap,
            layouts,
            field.field_type(),
            value,
            1,
            &mut budget,
            &mut memo,
        )?);
    }
    allocated.push(allocate_cookie_jar(
        heap,
        layouts,
        descriptor.fields()[RequestFieldProjection::COOKIE_JAR].field_type(),
        jar_cookie_entries,
        &mut budget,
        &mut memo,
    )?);
    let reference = heap
        .allocate_aggregate_ref(descriptor, &allocated)
        .map_err(|error| format!("error[execution_shard.managed_allocate]: {error}"))?;
    Ok(i64::from_ne_bytes(
        reference.encoded_abi_word().to_ne_bytes(),
    ))
}

fn allocate_cookie_jar(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    cookies: Vec<(String, String)>,
    budget: &mut usize,
    memo: &mut AllocationMemo,
) -> Result<ManagedFieldValue, String> {
    let ManagedFieldType::Reference(semantic) = field_type else {
        return Err(
            "error[execution_shard.http_request_projection]: CookieJar must be a managed reference"
                .to_string(),
        );
    };
    let descriptor = unique_tuple_layout(layouts, semantic, 2, "CookieJar")?;
    let values = [string_map(cookies), ReplValue::List(Vec::new())];
    let mut allocated = SmallVec::<[ManagedFieldValue; 2]>::new();
    for (field, value) in descriptor.fields().iter().zip(&values) {
        allocated.push(allocate_field(
            heap,
            layouts,
            field.field_type(),
            value,
            2,
            budget,
            memo,
        )?);
    }
    heap.allocate_aggregate_ref(descriptor, &allocated)
        .map(|reference| ManagedFieldValue::Reference(reference.erase()))
        .map_err(|error| format!("error[execution_shard.managed_allocate]: {error}"))
}

fn unique_tuple_layout<'a>(
    layouts: &'a ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    fields: usize,
    name: &str,
) -> Result<&'a ManagedAggregateDescriptor, String> {
    let mut candidates = layouts.layouts(semantic).iter().filter(|descriptor| {
        descriptor.kind() == ManagedAggregateKind::Tuple && descriptor.fields().len() == fields
    });
    let descriptor = candidates.next().ok_or_else(|| {
        format!("error[execution_shard.http_request_projection]: no admitted {name} layout matches")
    })?;
    if candidates.next().is_some() {
        return Err(format!(
            "error[execution_shard.http_request_projection]: admitted {name} layout is ambiguous"
        ));
    }
    Ok(descriptor)
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
    string_map(
        projection
            .requires(field)
            .then_some(entries)
            .unwrap_or_default(),
    )
}

fn string_map(entries: Vec<(String, String)>) -> ReplValue {
    ReplValue::Map(
        entries
            .into_iter()
            .map(|(key, value)| (ReplValue::String(key), ReplValue::String(value)))
            .collect(),
    )
}
