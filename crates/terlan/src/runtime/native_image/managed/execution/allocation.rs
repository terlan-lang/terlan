//! Managed allocation requests from generated native code.

use super::*;

/// Performs pointer checks and converts bounded ABI storage into Rust slices.
pub(super) fn managed_allocate_inner(
    context: *mut c_void,
    layout: *const u8,
    layout_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let Ok(layout_len) = usize::try_from(layout_len) else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    let Ok(field_count) = usize::try_from(field_count) else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    if context.is_null()
        || result.is_null()
        || layout.is_null()
        || !(context as *const ManagedAllocationContext).is_aligned()
        || !result.is_aligned()
        || layout_len == 0
        || layout_len > MAX_MANAGED_AGGREGATE_ABI_BYTES
        || field_count > MAX_AGGREGATE_FIELD_WORDS
        || (field_count != 0 && (fields.is_null() || !fields.is_aligned()))
    {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    }
    // SAFETY: The generated caller keeps all bounded buffers and the stack
    // context alive for this synchronous callback. Null and length checks above
    // establish the slice preconditions.
    let (context, layout, fields) = unsafe {
        let context = &mut *context.cast::<ManagedAllocationContext>();
        let layout = std::slice::from_raw_parts(layout, layout_len);
        let fields = if field_count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(fields, field_count)
        };
        (context, layout, fields)
    };
    // SAFETY: `with_dispatch` created this runtime pointer from an exclusive
    // borrow and does not expose it beyond the synchronous invocation.
    let runtime = unsafe { &mut *context.runtime };
    let allocation = (|| {
        runtime.heap(context.owner_id)?;
        let layouts = runtime.layouts.as_ref();
        let closure_dispatch = runtime.closure_dispatch.as_deref();
        let heap = runtime.heaps.get_mut(&context.owner_id).ok_or_else(|| {
            "error[managed_execution.heap]: actor heap insertion was lost".to_string()
        })?;
        heap.with_allocation_transaction(|heap| {
            if super::super::is_closure_allocation(layout) {
                let dispatch = closure_dispatch
                    .ok_or("managed closure allocation has no admitted image dispatch")?;
                super::super::execute_closure_allocation(heap, dispatch, layout, fields)
                    .map_err(|error| error.to_string())
            } else if super::super::is_managed_operation(layout) {
                super::super::execute_managed_operation(
                    heap,
                    layouts,
                    layout,
                    fields,
                )
                .map_err(|error| {
                    let family = std::str::from_utf8(layout.get(..4).unwrap_or_default())
                        .unwrap_or("invalid");
                    let operation = layout.get(6).copied().unwrap_or_default();
                    let semantic_context = match family {
                        "TVME" => {
                            let expected = layout.get(8..24).unwrap_or_default();
                            let actual = fields
                                .iter()
                                .enumerate()
                                .map(|(index, word)| {
                                    let reference = usize::try_from(u64::from_ne_bytes(
                                        word.to_ne_bytes(),
                                    ))
                                        .ok()
                                        .and_then(NonZeroUsize::new)
                                        .map(TvmRef::<()>::from_encoded);
                                    let resolved = reference
                                        .ok_or(super::super::ManagedMemoryError::UnknownReference)
                                        .and_then(|reference| heap.descriptor(reference))
                                        .map(|descriptor| descriptor.semantic_id().bytes())
                                        .map_err(|error| error.to_string());
                                    (index, resolved)
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "; expected semantic {expected:?}, operand words {fields:?}, actual semantics {actual:?}"
                            )
                        }
                        "TVMC" => {
                            let expected = layout.get(8..24).unwrap_or_default();
                            let actual = fields
                                .iter()
                                .filter_map(|word| {
                                    usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
                                        .ok()
                                        .and_then(NonZeroUsize::new)
                                })
                                .filter_map(|word| {
                                    heap.descriptor(TvmRef::<()>::from_encoded(word))
                                        .ok()
                                        .map(|descriptor| descriptor.semantic_id().bytes())
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "; expected collection semantic {expected:?}, operand words {fields:?}, actual reference semantics {actual:?}, admitted collections {:?}",
                                layouts.collection_inventory()
                            )
                        }
                        "TVMP" | "TVMO" => {
                            let expected_bytes = layout
                                .get(8..24)
                                .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
                                .unwrap_or_default();
                            let expected = super::super::SemanticTypeId::from_bytes(expected_bytes);
                            let actual = fields.first().map(|word| {
                                let encoded = u64::from_ne_bytes(word.to_ne_bytes());
                                let resolved = usize::try_from(encoded)
                                    .ok()
                                    .and_then(NonZeroUsize::new)
                                    .ok_or(super::super::ManagedMemoryError::UnknownReference)
                                    .and_then(|encoded| {
                                        heap.descriptor(TvmRef::<()>::from_encoded(encoded))
                                    })
                                    .map(|descriptor| {
                                        (
                                            descriptor.semantic_id().bytes(),
                                            descriptor.fingerprint(),
                                        )
                                    });
                                (encoded, resolved)
                            });
                            let admitted = layouts
                                .layouts(expected)
                                .iter()
                                .map(|descriptor| {
                                    (
                                        descriptor.canonical_type(),
                                        descriptor.variant_name(),
                                        descriptor.managed().fingerprint(),
                                    )
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "; expected aggregate semantic {expected_bytes:?}, actual {actual:?}, admitted layouts {admitted:?}"
                            )
                        }
                        _ => String::new(),
                    };
                    format!("{error}; managed operation {family}/{operation}{semantic_context}")
                })
            } else {
                heap.allocate_managed_words_abi(layout, fields)
                    .map_err(|error| {
                        let (heap_owner, heap_token, latest_retired_token) =
                            heap.diagnostic_identity();
                        let reference_tokens = fields
                            .iter()
                            .map(|word| u64::from_ne_bytes(word.to_ne_bytes()) >> 32)
                            .collect::<Vec<_>>();
                        let layout_context = super::super::decode_aggregate_layout(layout)
                            .ok()
                            .map(|descriptor| {
                                let supplied = descriptor
                                    .fields()
                                    .iter()
                                    .zip(fields)
                                    .map(|(field, word)| {
                                        (field.name(), field.field_type(), *word)
                                    })
                                    .collect::<Vec<_>>();
                                format!(
                                    "; aggregate {} supplied fields {supplied:?}",
                                    descriptor.canonical_type()
                                )
                            })
                            .unwrap_or_default();
                        format!(
                            "{error}{layout_context}; dispatch owner {}, heap owner {heap_owner}, heap token {heap_token}, latest retired token {latest_retired_token:?}, supplied upper words {reference_tokens:?}",
                            context.owner_id
                        )
                    })
            }
        })
    })()
    .map_err(|error| format!("error[managed_execution.allocate]: {error}"));
    match allocation {
        Ok(reference) => {
            if runtime
                .heap_ref(context.owner_id)
                .is_ok_and(ActorHeap::should_collect)
            {
                context.collection_requested = 1;
            }
            // SAFETY: Non-null caller-owned result storage remains live for the
            // callback and is written only after complete heap publication.
            unsafe { result.write(reference) };
            0
        }
        Err(error) => {
            runtime.retain_allocation_error(error);
            MANAGED_ALLOCATION_FAILED_STATUS
        }
    }
}
