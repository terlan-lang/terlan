//! Remaining session state operations over source-selected string identities.

use crate::runtime::vm::http_session::{self, VmHttpSession, VmHttpSessionService};

use super::super::{
    managed_string_semantic_id, ActorHeap, ManagedAggregateKind, ManagedFieldType,
    ManagedFieldValue, ManagedLayoutRegistry, ManagedMemoryError, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 4] = b"TVHS";
const VERSION: u16 = 2;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const CURRENT: u8 = 11;
const GET: u8 = 2;
const SET: u8 = 3;
const DELETE: u8 = 4;
const ROTATE: u8 = 12;
const EXPIRE: u8 = 6;
const IS_LIVE: u8 = 14;
const CURRENT_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 2;
const GET_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;

#[cfg(test)]
#[path = "session_cookie_test.rs"]
mod session_cookie_test;

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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn session_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded
        .get(6)
        .is_some_and(|operation| matches!(*operation, CURRENT | GET | ROTATE))
}

/// Encodes lookup or creation from a source-selected optional cookie value.
pub fn encode_session_current_operation(
    option_semantic: SemanticTypeId,
    result_semantic: SemanticTypeId,
) -> Vec<u8> {
    let mut encoded = header(CURRENT);
    push_semantics(&mut encoded, &[option_semantic, result_semantic]);
    encoded
}

/// Encodes one VM-owned session state read returning `Option[String]`.
pub fn encode_session_get_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(GET);
    push_semantics(&mut encoded, &[option_semantic]);
    encoded
}

/// Encodes one VM-owned session state mutation.
pub fn encode_session_mutation_operation(operation: ManagedSessionMutation) -> Vec<u8> {
    header(match operation {
        ManagedSessionMutation::Set => SET,
        ManagedSessionMutation::Delete => DELETE,
    })
}

/// Encodes session identity rotation while preserving actor-owned state.
pub fn encode_session_rotate_operation() -> Vec<u8> {
    header(ROTATE)
}

/// Encodes explicit session expiration and actor/table cleanup.
pub fn encode_session_expire_operation() -> Vec<u8> {
    header(EXPIRE)
}

/// Encodes a liveness query without observing the source session representation.
pub fn encode_session_is_live_operation() -> Vec<u8> {
    header(IS_LIVE)
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
                let [cookie] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let cookie = read_option_string(
                    heap,
                    layouts,
                    semantic_at(encoded, HEADER_BYTES)?,
                    *cookie,
                )?;
                let layout = super::unique_layout(
                    layouts,
                    semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                    2,
                )?;
                if layout.kind() != ManagedAggregateKind::Tuple
                    || layout.fields()[0].field_type()
                        != ManagedFieldType::Reference(managed_string_semantic_id())
                    || layout.fields()[1].field_type() != ManagedFieldType::Bool
                {
                    return Err(ManagedMemoryError::ManagedTypeMismatch);
                }
                let lookup = http_session::current(sessions, cookie.as_deref())
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                let (session, pending) = lookup.into_managed_parts();
                let identity = heap.allocate_string(session.managed_id())?;
                heap.allocate_aggregate_ref(
                    layout,
                    &[
                        ManagedFieldValue::Reference(identity.erase()),
                        ManagedFieldValue::Bool(pending.is_some()),
                    ],
                )
                .map(TvmRef::encoded_abi_word)
            }
            GET if encoded.len() == GET_BYTES => {
                let [session, key] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session = read_session(heap, *session)?;
                let key = managed_string(heap, *key)?;
                let value = http_session::get(sessions, &session, &key)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                allocate_option_string(
                    heap,
                    layouts,
                    semantic_at(encoded, HEADER_BYTES)?,
                    value.as_deref(),
                )
            }
            SET if encoded.len() == HEADER_BYTES => {
                let [session, key, value] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session = read_session(heap, *session)?;
                http_session::set(
                    sessions,
                    &session,
                    &managed_string(heap, *key)?,
                    &managed_string(heap, *value)?,
                )
                .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                Ok(0)
            }
            DELETE if encoded.len() == HEADER_BYTES => {
                let [session, key] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session = read_session(heap, *session)?;
                http_session::delete(sessions, &session, &managed_string(heap, *key)?)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                Ok(0)
            }
            ROTATE if encoded.len() == HEADER_BYTES => {
                let [session] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session = read_session(heap, *session)?;
                let lookup = http_session::rotate(sessions, &session)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                let (session, _) = lookup.into_managed_parts();
                heap.allocate_string(session.managed_id())
                    .map(TvmRef::encoded_abi_word)
            }
            EXPIRE if encoded.len() == HEADER_BYTES => {
                let [session] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session = read_session(heap, *session)?;
                http_session::expire(sessions, &session)
                    .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
                Ok(0)
            }
            IS_LIVE if encoded.len() == HEADER_BYTES => {
                let [session] = words else {
                    return Err(ManagedMemoryError::InvalidAggregateArity);
                };
                let session = read_session(heap, *session)?;
                Ok(u64::from(sessions.is_live(&session)))
            }
            _ => Err(ManagedMemoryError::InvalidManagedOperation),
        })
        .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?
}

/// Decodes only the identity selected by the source-owned session record.
fn read_session(heap: &ActorHeap, word: i64) -> Result<VmHttpSession, ManagedMemoryError> {
    managed_string(heap, word).map(VmHttpSession::from_managed_id)
}

/// Reads the checked source option without depending on a request representation.
fn read_option_string(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    option: i64,
) -> Result<Option<String>, ManagedMemoryError> {
    let option = super::reference_word(option)?;
    let layout = layouts
        .layout_for_reference(heap, semantic, option)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let fields = super::aggregate_fields(heap, layout, option)?;
    match (layout.variant_name(), fields.as_slice()) {
        (Some("Some"), [ManagedFieldValue::Reference(value)]) => {
            Ok(Some(heap.read_string(value.cast())?.to_string()))
        }
        (Some("None"), []) => Ok(None),
        _ => Err(ManagedMemoryError::ManagedTypeMismatch),
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

/// Builds one canonical session operation header.
fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(CURRENT_BYTES);
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
