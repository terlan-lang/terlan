//! Bounded HTTP operations over actor-owned managed values.

use crate::terlan_native::http as native_http;

use super::super::{
    ActorHeap, ManagedAggregate, ManagedFieldValue, ManagedLayoutRegistry, ManagedList,
    ManagedMemoryError, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 4] = b"TVHO";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const COOKIE_HEADER: u8 = 1;
const RESPONSE_COOKIE_JAR: u8 = 2;
const RESPONSE_SECURITY_HEADERS: u8 = 3;
const RESPONSE_BUILD: u8 = 4;
const COOKIE_BYTES: usize = HEADER_BYTES;
const COOKIE_JAR_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 5 + 8;
const SECURITY_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 4 + 4;
const RESPONSE_BUILD_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 2;

/// Closed maintained-cookie serializer operation selected by generated code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedCookieHeaderOperation {
    /// Serializes name, value, path, HttpOnly, and Secure.
    Set,
    /// Serializes the complete supported cookie option surface.
    SetWithOptions,
    /// Serializes an expiring cookie deletion.
    Delete,
}

/// Reports whether bytes identify the managed HTTP operation family.
pub(super) fn is_http_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Encodes one maintained-cookie header serialization operation.
pub fn encode_cookie_header_operation(operation: ManagedCookieHeaderOperation) -> Vec<u8> {
    let mut encoded = header(COOKIE_HEADER);
    encoded[7] = match operation {
        ManagedCookieHeaderOperation::Set => 1,
        ManagedCookieHeaderOperation::SetWithOptions => 2,
        ManagedCookieHeaderOperation::Delete => 3,
    };
    encoded
}

/// Encodes immutable replay of a managed cookie jar into response headers.
#[allow(clippy::too_many_arguments)]
pub fn encode_response_cookie_jar_operation(
    response_semantic: SemanticTypeId,
    header_list_semantic: SemanticTypeId,
    header_semantic: SemanticTypeId,
    jar_semantic: SemanticTypeId,
    mutation_list_semantic: SemanticTypeId,
    response_header_field: usize,
    jar_mutation_field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let response_header_field = u32::try_from(response_header_field)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let jar_mutation_field =
        u32::try_from(jar_mutation_field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(RESPONSE_COOKIE_JAR);
    for semantic in [
        response_semantic,
        header_list_semantic,
        header_semantic,
        jar_semantic,
        mutation_list_semantic,
    ] {
        encoded.extend_from_slice(&semantic.bytes());
    }
    encoded.extend_from_slice(&response_header_field.to_le_bytes());
    encoded.extend_from_slice(&jar_mutation_field.to_le_bytes());
    Ok(encoded)
}

/// Encodes immutable application of a typed security policy to a response.
pub fn encode_response_security_headers_operation(
    response_semantic: SemanticTypeId,
    header_list_semantic: SemanticTypeId,
    header_semantic: SemanticTypeId,
    policy_semantic: SemanticTypeId,
    response_header_field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let response_header_field = u32::try_from(response_header_field)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(RESPONSE_SECURITY_HEADERS);
    for semantic in [
        response_semantic,
        header_list_semantic,
        header_semantic,
        policy_semantic,
    ] {
        encoded.extend_from_slice(&semantic.bytes());
    }
    encoded.extend_from_slice(&response_header_field.to_le_bytes());
    Ok(encoded)
}

/// Encodes construction of one ordinary non-file HTTP response.
pub fn encode_response_build_operation(
    response_semantic: SemanticTypeId,
    header_list_semantic: SemanticTypeId,
    kind: u8,
) -> Result<Vec<u8>, ManagedMemoryError> {
    if kind > 3 {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let mut encoded = header(RESPONSE_BUILD);
    encoded[7] = kind;
    encoded.extend_from_slice(&response_semantic.bytes());
    encoded.extend_from_slice(&header_list_semantic.bytes());
    Ok(encoded)
}

/// Executes one exact HTTP operation descriptor against an actor heap.
pub(super) fn execute_http_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate_header(encoded)?;
    let reference = match encoded[6] {
        COOKIE_HEADER if encoded.len() == COOKIE_BYTES => {
            serialize_cookie(heap, encoded[7], words)?.erase()
        }
        RESPONSE_COOKIE_JAR if encoded.len() == COOKIE_JAR_BYTES && encoded[7] == 0 => {
            let [response, jar] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            apply_cookie_jar(heap, layouts, encoded, *response, *jar)?
        }
        RESPONSE_SECURITY_HEADERS if encoded.len() == SECURITY_BYTES && encoded[7] == 0 => {
            let [response, policy] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            apply_security_headers(heap, layouts, encoded, *response, *policy)?
        }
        RESPONSE_BUILD if encoded.len() == RESPONSE_BUILD_BYTES && encoded[7] <= 3 => {
            let [payload, status] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            build_response(heap, layouts, encoded, *payload, *status)?
        }
        _ => return Err(ManagedMemoryError::InvalidManagedOperation),
    };
    Ok(reference.encoded_abi_word())
}

/// Allocates the uniform Response envelope and its empty defaults in one ABI call.
fn build_response(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    payload: i64,
    status: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let response_semantic = semantic_at(encoded, HEADER_BYTES)?;
    let headers_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
    let [response] = layouts.layouts(response_semantic) else {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    };
    let headers = layouts
        .collection(headers_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let empty_path = heap.allocate_string("")?.erase();
    let empty_headers = heap.list_from_elements(headers, &[])?.erase();
    let payload = super::reference_word(payload)?;
    heap.allocate_aggregate_ref(
        response,
        &[
            ManagedFieldValue::Int(0),
            ManagedFieldValue::Int(i64::from(encoded[7])),
            ManagedFieldValue::Reference(payload),
            ManagedFieldValue::Int(status),
            ManagedFieldValue::Reference(empty_path),
            ManagedFieldValue::Reference(empty_headers),
        ],
    )
    .map(TvmRef::<ManagedAggregate>::erase)
}

/// Validates the common HTTP operation header.
fn validate_header(encoded: &[u8]) -> Result<(), ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

/// Serializes one cookie header through the maintained HTTP adapter.
fn serialize_cookie(
    heap: &mut ActorHeap,
    operation: u8,
    words: &[i64],
) -> Result<TvmRef<super::super::ManagedString>, ManagedMemoryError> {
    let header = match (operation, words) {
        (1, [name, value, path, http_only, secure]) => native_http::set_header(
            &managed_string(heap, *name)?,
            &managed_string(heap, *value)?,
            &managed_string(heap, *path)?,
            managed_bool(*http_only)?,
            managed_bool(*secure)?,
        ),
        (
            2,
            [name, value, path, domain, max_age, include_max_age, expires, http_only, secure, same_site],
        ) => {
            let domain = managed_string(heap, *domain)?;
            let expires = managed_string(heap, *expires)?;
            let same_site = managed_string(heap, *same_site)?;
            let mut options = native_http::CookieOptions::defaults();
            options.path = managed_string(heap, *path)?;
            options.domain = non_empty(domain);
            options.max_age = managed_bool(*include_max_age)?.then_some(*max_age);
            options.expires = non_empty(expires);
            options.http_only = managed_bool(*http_only)?;
            options.secure = managed_bool(*secure)?;
            options.same_site = cookie_same_site(&same_site)?;
            native_http::set_header_with_options(
                &managed_string(heap, *name)?,
                &managed_string(heap, *value)?,
                &options,
            )
        }
        (3, [name, path]) => native_http::delete_header(
            &managed_string(heap, *name)?,
            &managed_string(heap, *path)?,
        ),
        _ => return Err(ManagedMemoryError::InvalidAggregateArity),
    }
    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
    heap.allocate_string(&header)
}

/// Replays every persistent jar mutation as one repeated `Set-Cookie` header.
fn apply_cookie_jar(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    response: i64,
    jar: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let response_semantic = semantic_at(encoded, HEADER_BYTES)?;
    let header_list_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
    let header_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?;
    let jar_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 3)?;
    let mutation_list_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 4)?;
    let response_field = field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 5)?;
    let jar_field = field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 5 + 4)?;
    let jar = super::reference_word(jar)?;
    let jar_layout = layouts
        .layout_for_reference(heap, jar_semantic, jar)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let jar_values = super::aggregate_fields(heap, &jar_layout, jar)?;
    let mutations = match jar_values.get(jar_field) {
        Some(ManagedFieldValue::Reference(reference)) => reference.cast::<ManagedList>(),
        _ => return Err(ManagedMemoryError::InvalidAggregateField),
    };
    let descriptor = layouts
        .collection(mutation_list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let mutations = heap.list_elements(descriptor, mutations)?;
    append_headers(
        heap,
        layouts,
        response_semantic,
        header_list_semantic,
        header_semantic,
        response_field,
        response,
        mutations
            .into_iter()
            .map(|value| match value {
                ManagedFieldValue::Reference(value) => {
                    heap.read_string(value.cast()).map(str::to_owned)
                }
                _ => Err(ManagedMemoryError::InvalidAggregateField),
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|value| ("Set-Cookie".to_string(), value)),
    )
}

/// Renders one admitted typed security policy into persistent response headers.
fn apply_security_headers(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    response: i64,
    policy: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let response_semantic = semantic_at(encoded, HEADER_BYTES)?;
    let header_list_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
    let header_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?;
    let policy_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 3)?;
    let response_field = field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 4)?;
    let policy = super::reference_word(policy)?;
    let policy_layout = layouts
        .layout_for_reference(heap, policy_semantic, policy)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let values = super::aggregate_fields(heap, &policy_layout, policy)?;
    let [ManagedFieldValue::Bool(content_type_options), ManagedFieldValue::Int(frame_options), ManagedFieldValue::Int(referrer_policy), ManagedFieldValue::Int(hsts_max_age), ManagedFieldValue::Bool(hsts_include_subdomains)] =
        values.as_slice()
    else {
        return Err(ManagedMemoryError::InvalidAggregateField);
    };
    let frame_options = match frame_options {
        0 => "DENY",
        1 => "SAMEORIGIN",
        _ => return Err(ManagedMemoryError::InvalidManagedOperation),
    };
    let referrer_policy = match referrer_policy {
        0 => "no-referrer",
        1 => "strict-origin-when-cross-origin",
        _ => return Err(ManagedMemoryError::InvalidManagedOperation),
    };
    let mut headers = vec![
        ("X-Frame-Options".to_string(), frame_options.to_string()),
        ("Referrer-Policy".to_string(), referrer_policy.to_string()),
    ];
    if *content_type_options {
        headers.push(("X-Content-Type-Options".to_string(), "nosniff".to_string()));
    }
    if *hsts_max_age > 0 {
        let suffix = if *hsts_include_subdomains {
            "; includeSubDomains"
        } else {
            ""
        };
        headers.push((
            "Strict-Transport-Security".to_string(),
            format!("max-age={hsts_max_age}{suffix}"),
        ));
    }
    append_headers(
        heap,
        layouts,
        response_semantic,
        header_list_semantic,
        header_semantic,
        response_field,
        response,
        headers.into_iter(),
    )
}

/// Appends owned name/value pairs through the generic persistent response primitive.
#[allow(clippy::too_many_arguments)]
pub(super) fn append_headers(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    response_semantic: SemanticTypeId,
    header_list_semantic: SemanticTypeId,
    header_semantic: SemanticTypeId,
    response_field: usize,
    response: i64,
    headers: impl IntoIterator<Item = (String, String)>,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let mut response = response;
    for (name, value) in headers {
        let name = heap.allocate_string(&name)?;
        let value = heap.allocate_string(&value)?;
        let updated = super::append_pair_to_aggregate_list(
            heap,
            layouts,
            response_semantic,
            header_list_semantic,
            header_semantic,
            response_field,
            response,
            reference_word(name.erase()),
            reference_word(value.erase()),
        )?;
        response = reference_word(updated);
    }
    super::reference_word(response)
}

/// Reads one owned managed string from a native argument word.
fn managed_string(heap: &ActorHeap, word: i64) -> Result<String, ManagedMemoryError> {
    heap.read_string(super::reference_word(word)?.cast())
        .map(str::to_owned)
}

/// Decodes one canonical Boolean ABI word.
fn managed_bool(word: i64) -> Result<bool, ManagedMemoryError> {
    match word {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ManagedMemoryError::InvalidManagedScalar),
    }
}

/// Converts empty option text into absence.
fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

/// Converts the public SameSite vocabulary into maintained adapter values.
fn cookie_same_site(
    value: &str,
) -> Result<Option<native_http::CookieSameSite>, ManagedMemoryError> {
    match value {
        "" => Ok(None),
        "lax" => Ok(Some(native_http::CookieSameSite::Lax)),
        "strict" => Ok(Some(native_http::CookieSameSite::Strict)),
        "none" => Ok(Some(native_http::CookieSameSite::None)),
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

/// Builds the common immutable HTTP operation header.
fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(HEADER_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

/// Reads one semantic identity from exact operation bytes.
fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

/// Reads one physical field index from exact operation bytes.
fn field_at(encoded: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    encoded
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

/// Converts one managed reference into its signed callback word.
fn reference_word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

#[cfg(test)]
#[path = "http_test.rs"]
mod http_test;
