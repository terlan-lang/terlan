//! VM-owned HTTP session operations over opaque managed handles.

use crate::runtime::vm::http_session::{self, VmHttpSession, VmHttpSessionService};
use crate::terlan_native::http as native_http;

use super::super::{
    ActorHeap, ManagedFieldValue, ManagedLayoutRegistry, ManagedMap, ManagedMemoryError,
    ManagedStringKeySemantics, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 4] = b"TVHS";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const CURRENT: u8 = 1;
const GET: u8 = 2;
const SET: u8 = 3;
const DELETE: u8 = 4;
const ROTATE: u8 = 5;
const EXPIRE: u8 = 6;
const WITH_RESPONSE: u8 = 7;
const OPTION_IS_NONE: u8 = 8;
const CURRENT_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 3 + 4;
const GET_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 2;
const SESSION_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;
const RESPONSE_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 4 + 4;
const SESSION_COOKIE_NAME: &str = "terlan_session";

/// VM session state mutation selected by generated native code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedSessionMutation {
    /// Stores one string value under a string key.
    Set,
    /// Removes one string value by key.
    Delete,
}

/// Reports whether bytes identify the managed session operation family.
pub(super) fn is_session_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Reports whether one managed session operation returns an opaque reference.
pub(super) fn session_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded
        .get(6)
        .is_some_and(|operation| matches!(*operation, CURRENT | GET | ROTATE | WITH_RESPONSE))
}

/// Encodes request-cookie lookup or creation of one VM-owned session actor.
pub fn encode_session_current_operation(
    request_semantic: SemanticTypeId,
    cookie_map_semantic: SemanticTypeId,
    session_semantic: SemanticTypeId,
    request_cookie_field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field =
        u32::try_from(request_cookie_field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(CURRENT);
    push_semantics(
        &mut encoded,
        &[request_semantic, cookie_map_semantic, session_semantic],
    );
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Encodes one VM-owned session state read returning `Option[String]`.
pub fn encode_session_get_operation(
    session_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    let mut encoded = header(GET);
    push_semantics(&mut encoded, &[session_semantic, option_semantic]);
    encoded
}

/// Encodes a checked `None` predicate for one managed option value.
pub fn encode_session_option_is_none_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(OPTION_IS_NONE);
    push_semantics(&mut encoded, &[option_semantic]);
    encoded
}

/// Encodes one VM-owned session state mutation.
pub fn encode_session_mutation_operation(
    operation: ManagedSessionMutation,
    session_semantic: SemanticTypeId,
) -> Vec<u8> {
    let mut encoded = header(match operation {
        ManagedSessionMutation::Set => SET,
        ManagedSessionMutation::Delete => DELETE,
    });
    push_semantics(&mut encoded, &[session_semantic]);
    encoded
}

/// Encodes session identity rotation while preserving actor-owned state.
pub fn encode_session_rotate_operation(session_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(ROTATE);
    push_semantics(&mut encoded, &[session_semantic]);
    encoded
}

/// Encodes explicit session expiration and actor/table cleanup.
pub fn encode_session_expire_operation(session_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(EXPIRE);
    push_semantics(&mut encoded, &[session_semantic]);
    encoded
}

/// Encodes explicit threading of session cookie state onto a response.
pub fn encode_session_with_response_operation(
    session_semantic: SemanticTypeId,
    response_semantic: SemanticTypeId,
    header_list_semantic: SemanticTypeId,
    header_semantic: SemanticTypeId,
    response_header_field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(response_header_field)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(WITH_RESPONSE);
    push_semantics(
        &mut encoded,
        &[
            session_semantic,
            response_semantic,
            header_list_semantic,
            header_semantic,
        ],
    );
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Executes one checked session operation against the shared VM request service.
pub(super) fn execute_session_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    sessions: Option<&VmHttpSessionService>,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate(encoded)?;
    let sessions = sessions.ok_or(ManagedMemoryError::InvalidManagedOperation)?;
    sessions
        .with_runtime(|sessions| match encoded[6] {
            CURRENT if encoded.len() == CURRENT_BYTES => {
                let [request] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let cookie = request_cookie(heap, layouts, encoded, *request)?;
                let lookup = http_session::current(sessions, cookie.as_deref())
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                let (session, pending_cookie) = lookup.into_managed_parts();
                allocate_session(
                    heap,
                    layouts,
                    semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?,
                    session.managed_id(),
                    pending_cookie.as_deref().unwrap_or(""),
                )
            }
            GET if encoded.len() == GET_BYTES => {
                let [session, key] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session =
                    read_session(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *session)?;
                let key = managed_string(heap, *key)?;
                let value = http_session::get(sessions, &session.handle, &key)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                allocate_option_string(
                    heap,
                    layouts,
                    semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                    value.as_deref(),
                )
            }
            SET if encoded.len() == SESSION_BYTES => {
                let [session, key, value] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session =
                    read_session(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *session)?;
                http_session::set(
                    sessions,
                    &session.handle,
                    &managed_string(heap, *key)?,
                    &managed_string(heap, *value)?,
                )
                .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                Ok(0)
            }
            DELETE if encoded.len() == SESSION_BYTES => {
                let [session, key] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session =
                    read_session(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *session)?;
                http_session::delete(sessions, &session.handle, &managed_string(heap, *key)?)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                Ok(0)
            }
            ROTATE if encoded.len() == SESSION_BYTES => {
                let [session] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let semantic = semantic_at(encoded, HEADER_BYTES)?;
                let session = read_session(heap, layouts, semantic, *session)?;
                let lookup = http_session::rotate(sessions, &session.handle)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                let (session, pending_cookie) = lookup.into_managed_parts();
                allocate_session(
                    heap,
                    layouts,
                    semantic,
                    session.managed_id(),
                    pending_cookie.as_deref().unwrap_or(""),
                )
            }
            EXPIRE if encoded.len() == SESSION_BYTES => {
                let [session] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session =
                    read_session(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *session)?;
                http_session::expire(sessions, &session.handle)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                Ok(0)
            }
            WITH_RESPONSE if encoded.len() == RESPONSE_BYTES => {
                let [response, session] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session =
                    read_session(heap, layouts, semantic_at(encoded, HEADER_BYTES)?, *session)?;
                let cookie = if sessions.is_live(&session.handle) {
                    (!session.pending_cookie.is_empty()).then_some(session.pending_cookie)
                } else {
                    Some(
                        native_http::delete_header(SESSION_COOKIE_NAME, "/")
                            .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?,
                    )
                };
                let Some(cookie) = cookie else {
                    return super::reference_word(*response).map(TvmRef::encoded_abi_word);
                };
                super::http::append_headers(
                    heap,
                    layouts,
                    semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                    semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?,
                    semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 3)?,
                    field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 4)?,
                    *response,
                    [("Set-Cookie".to_string(), cookie)],
                )
                .map(TvmRef::encoded_abi_word)
            }
            OPTION_IS_NONE if encoded.len() == SESSION_BYTES => {
                let [option] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let option = super::reference_word(*option)?;
                let layout = layouts
                    .layout_for_reference(heap, semantic_at(encoded, HEADER_BYTES)?, option)
                    .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
                match layout.variant_name() {
                    Some("None") => Ok(1),
                    Some("Some") => Ok(0),
                    _ => Err(ManagedMemoryError::ManagedTypeMismatch),
                }
            }
            _ => Err(ManagedMemoryError::InvalidManagedOperation),
        })
        .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?
}

/// Decoded actor-owned session identity and response metadata.
struct ManagedSessionValue {
    /// Opaque handle used to address the VM session actor.
    handle: VmHttpSession,
    /// Serialized cookie awaiting explicit response threading.
    pending_cookie: String,
}

/// Allocates one opaque session value in the current actor heap.
fn allocate_session(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    id: &str,
    pending_cookie: &str,
) -> Result<u64, ManagedMemoryError> {
    let id = heap.allocate_string(id)?;
    let pending = heap.allocate_string(pending_cookie)?;
    let layout = super::unique_layout(layouts, semantic, 2)?;
    heap.allocate_aggregate_ref(
        layout,
        &[
            ManagedFieldValue::Reference(id.erase()),
            ManagedFieldValue::Reference(pending.erase()),
        ],
    )
    .map(TvmRef::encoded_abi_word)
}

/// Decodes one checked managed session value into runtime-owned data.
fn read_session(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    word: i64,
) -> Result<ManagedSessionValue, ManagedMemoryError> {
    let reference = super::reference_word(word)?;
    let layout = layouts
        .layout_for_reference(heap, semantic, reference)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let fields = super::aggregate_fields(heap, &layout, reference)?;
    let [ManagedFieldValue::Reference(id), ManagedFieldValue::Reference(pending)] =
        fields.as_slice()
    else {
        return Err(ManagedMemoryError::InvalidAggregateField);
    };
    Ok(ManagedSessionValue {
        handle: VmHttpSession::from_managed_id(heap.read_string(id.cast())?.to_string()),
        pending_cookie: heap.read_string(pending.cast())?.to_string(),
    })
}

/// Reads the reserved session cookie from one managed request map.
fn request_cookie(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    request: i64,
) -> Result<Option<String>, ManagedMemoryError> {
    let request = super::reference_word(request)?;
    let layout = layouts
        .layout_for_reference(heap, semantic_at(encoded, HEADER_BYTES)?, request)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let fields = super::aggregate_fields(heap, &layout, request)?;
    let map = match fields.get(field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 3)?) {
        Some(ManagedFieldValue::Reference(map)) => map.cast::<ManagedMap>(),
        _ => return Err(ManagedMemoryError::InvalidAggregateField),
    };
    let descriptor = layouts
        .collection(semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?)
        .and_then(|collection| collection.map_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let key = heap.allocate_string(SESSION_COOKIE_NAME)?;
    match heap.map_get(
        descriptor,
        map,
        ManagedFieldValue::Reference(key.erase()),
        &mut ManagedStringKeySemantics,
    )? {
        Some(ManagedFieldValue::Reference(value)) => {
            Ok(Some(heap.read_string(value.cast())?.to_string()))
        }
        Some(_) => Err(ManagedMemoryError::ManagedTypeMismatch),
        None => Ok(None),
    }
}

/// Allocates the active managed `Option[String]` constructor.
fn allocate_option_string(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: Option<&str>,
) -> Result<u64, ManagedMemoryError> {
    let (variant, fields) = match value {
        Some(value) => {
            let value = heap.allocate_string(value)?;
            ("Some", vec![ManagedFieldValue::Reference(value.erase())])
        }
        None => ("None", Vec::new()),
    };
    let layout = super::option_layout(layouts, semantic, variant, fields.len())?;
    heap.allocate_aggregate_ref(layout, &fields)
        .map(TvmRef::encoded_abi_word)
}

/// Copies one checked managed string argument into runtime-owned text.
fn managed_string(heap: &ActorHeap, word: i64) -> Result<String, ManagedMemoryError> {
    heap.read_string(super::reference_word(word)?.cast())
        .map(str::to_owned)
}

/// Validates the common session operation header.
fn validate(encoded: &[u8]) -> Result<(), ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] != 0
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

/// Decodes one semantic identity from an admitted operation payload.
fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| bytes.try_into().ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)
}

/// Decodes one bounded aggregate field index.
fn field_at(encoded: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    encoded
        .get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .map(|field| field as usize)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)
}

/// Builds one canonical session operation header.
fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(RESPONSE_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

/// Appends semantic identities in canonical descriptor order.
fn push_semantics(encoded: &mut Vec<u8>, semantics: &[SemanticTypeId]) {
    for semantic in semantics {
        encoded.extend_from_slice(&semantic.bytes());
    }
}
