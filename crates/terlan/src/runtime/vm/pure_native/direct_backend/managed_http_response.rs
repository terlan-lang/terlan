//! Direct extraction of the fixed managed HTTP Response envelope.

use std::num::NonZeroUsize;
use std::sync::OnceLock;

use crate::runtime::native_image::managed::{
    ManagedAggregate, ManagedAggregateDescriptor, ManagedExecutionRuntime, ManagedFieldValue,
    ManagedLayoutRegistry, ManagedList, ManagedListDescriptor, ManagedString, SemanticTypeId,
    TvmRef,
};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::VmAotHttpResponse;

const RESPONSE: &str = "Named(Response)";
const RESPONSE_HEADERS: &str = "std.http.Response.Headers";
const RESPONSE_HEADER: &str = "std.http.Response.Header";

/// Admission-time projection of the fixed HTTP response layouts.
///
/// Images without the standard response types keep the generic lookup path.
/// The common HTTP image resolves these immutable descriptors once instead of
/// traversing registry maps for every completed request.
pub(super) struct HttpResponseSchema {
    response: Option<ManagedAggregateDescriptor>,
    headers: Option<ManagedListDescriptor>,
    header: Option<ManagedAggregateDescriptor>,
}

impl HttpResponseSchema {
    pub(super) fn admit(layouts: &ManagedLayoutRegistry) -> Self {
        Self {
            response: unique_layout(layouts, response_semantic()),
            headers: layouts
                .collection(headers_semantic())
                .and_then(|collection| collection.list_descriptor())
                .cloned(),
            header: unique_layout(layouts, header_semantic()),
        }
    }
}

/// Extracts a non-file response directly from its actor heap. `None` retains
/// the generic materializer for non-Response results and file responses.
pub(super) fn materialize_http_response(
    managed: &ManagedExecutionRuntime,
    schema: &HttpResponseSchema,
    owner_id: u64,
    result_type: &TvmBoundaryType,
    word: i64,
) -> Result<Option<VmAotHttpResponse>, String> {
    let response_semantic = response_semantic();
    if result_type != &TvmBoundaryType::Managed(response_semantic.bytes()) {
        return Ok(None);
    }
    managed.validate_boundary_reference(owner_id, result_type, word)?;
    managed.with_public_materialization(owner_id, |heap, layouts| {
        let response = reference_word(word)?;
        let descriptor = match schema.response.as_ref() {
            Some(descriptor) => descriptor,
            None => layouts.layout_for_reference(heap, response_semantic, response)?,
        };
        let view = heap
            .read_aggregate(response.cast::<ManagedAggregate>(), descriptor)
            .map_err(memory_error)?;
        let tag = int_field(&view, 0)?;
        let kind = int_field(&view, 1)?;
        if tag != 0 {
            return Err("error[execution_shard.http_response]: invalid Response tag".to_string());
        }
        if kind == 4 {
            return Ok(None);
        }
        if !(0..=3).contains(&kind) {
            return Err(format!(
                "error[execution_shard.http_response]: unsupported Response kind `{kind}`"
            ));
        }
        let payload = string_field(heap, &view, 2)?;
        let status = int_field(&view, 3)?;
        let headers = header_fields(heap, layouts, schema, &view)?;
        Ok(Some(VmAotHttpResponse {
            kind,
            status,
            payload,
            headers,
        }))
    })
}

fn header_fields(
    heap: &crate::runtime::native_image::managed::ActorHeap,
    layouts: &crate::runtime::native_image::managed::ManagedLayoutRegistry,
    schema: &HttpResponseSchema,
    response: &crate::runtime::native_image::managed::ManagedAggregateView<'_>,
) -> Result<Vec<(String, String)>, String> {
    let ManagedFieldValue::Reference(headers) = response.field(5).map_err(memory_error)? else {
        return Err("error[execution_shard.http_response]: headers are not a reference".into());
    };
    let headers_semantic = headers_semantic();
    let descriptor = match schema.headers.as_ref() {
        Some(descriptor) => descriptor,
        None => layouts
            .collection(headers_semantic)
            .and_then(|collection| collection.list_descriptor())
            .ok_or_else(|| {
                "error[execution_shard.http_response]: Response header list layout is missing"
                    .to_string()
            })?,
    };
    let headers = headers.cast::<ManagedList>();
    let length = heap
        .list_length(descriptor, headers)
        .map_err(memory_error)?;
    let header_semantic = header_semantic();
    let mut result = Vec::with_capacity(length);
    for index in 0..length {
        let ManagedFieldValue::Reference(header) = heap
            .list_get(descriptor, headers, index)
            .map_err(memory_error)?
        else {
            return Err("error[execution_shard.http_response]: malformed header entry".into());
        };
        let layout = match schema.header.as_ref() {
            Some(layout) => layout,
            None => layouts.layout_for_reference(heap, header_semantic, header)?,
        };
        let header = heap
            .read_aggregate(header.cast::<ManagedAggregate>(), layout)
            .map_err(memory_error)?;
        result.push((
            string_field(heap, &header, 0)?,
            string_field(heap, &header, 1)?,
        ));
    }
    Ok(result)
}

fn int_field(
    aggregate: &crate::runtime::native_image::managed::ManagedAggregateView<'_>,
    index: usize,
) -> Result<i64, String> {
    match aggregate.field(index).map_err(memory_error)? {
        ManagedFieldValue::Int(value) => Ok(value),
        _ => Err(format!(
            "error[execution_shard.http_response]: field {index} is not Int"
        )),
    }
}

fn string_field(
    heap: &crate::runtime::native_image::managed::ActorHeap,
    aggregate: &crate::runtime::native_image::managed::ManagedAggregateView<'_>,
    index: usize,
) -> Result<String, String> {
    let ManagedFieldValue::Reference(value) = aggregate.field(index).map_err(memory_error)? else {
        return Err(format!(
            "error[execution_shard.http_response]: field {index} is not String"
        ));
    };
    heap.read_string(value.cast::<ManagedString>())
        .map(str::to_owned)
        .map_err(memory_error)
}

fn reference_word(word: i64) -> Result<TvmRef<()>, String> {
    usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
        .ok()
        .and_then(NonZeroUsize::new)
        .map(TvmRef::from_encoded)
        .ok_or_else(|| "error[execution_shard.http_response]: invalid reference word".to_string())
}

fn response_semantic() -> SemanticTypeId {
    static SEMANTIC: OnceLock<SemanticTypeId> = OnceLock::new();
    *SEMANTIC.get_or_init(|| canonical_semantic(RESPONSE))
}

fn headers_semantic() -> SemanticTypeId {
    static SEMANTIC: OnceLock<SemanticTypeId> = OnceLock::new();
    *SEMANTIC.get_or_init(|| canonical_semantic(RESPONSE_HEADERS))
}

fn header_semantic() -> SemanticTypeId {
    static SEMANTIC: OnceLock<SemanticTypeId> = OnceLock::new();
    *SEMANTIC.get_or_init(|| canonical_semantic(RESPONSE_HEADER))
}

fn canonical_semantic(canonical: &'static str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical)
        .expect("compiler-owned HTTP response semantic identity is valid")
}

fn unique_layout(
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
) -> Option<ManagedAggregateDescriptor> {
    let layouts = layouts.layouts(semantic);
    (layouts.len() == 1).then(|| layouts[0].as_ref().clone())
}

fn memory_error(error: impl std::fmt::Display) -> String {
    format!("error[execution_shard.http_response]: {error}")
}
