/// Validates one generated allocation call and publishes through its owner heap.
#[allow(unsafe_code)]
pub(crate) unsafe extern "C" fn managed_allocate(
    context: *mut c_void,
    layout: *const u8,
    layout_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        managed_allocate_inner(context, layout, layout_len, fields, field_count, result)
    }))
    .unwrap_or(MANAGED_ALLOCATION_FAILED_STATUS)
}

/// Resolves one owned closure into its stable image target and validated ABI words.
#[allow(unsafe_code, clippy::too_many_arguments)]
pub(crate) unsafe extern "C" fn managed_resolve_closure(
    context: *mut c_void,
    closure: i64,
    parameter_type_words: *const i64,
    parameter_count: u64,
    parameter_words: *const i64,
    result_type_words: *const i64,
    result_count: u64,
    target: *mut u64,
    invocation_words: *mut i64,
    invocation_capacity: u64,
    invocation_len: *mut u64,
) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        resolve_closure_inner(
            context,
            closure,
            parameter_type_words,
            parameter_count,
            parameter_words,
            result_type_words,
            result_count,
            target,
            invocation_words,
            invocation_capacity,
            invocation_len,
        )
    }))
    .unwrap_or(MANAGED_ALLOCATION_FAILED_STATUS)
}

/// Checks all raw resolver buffers before entering actor-owned managed state.
#[allow(unsafe_code, clippy::too_many_arguments)]
fn resolve_closure_inner(
    context: *mut c_void,
    closure: i64,
    parameter_type_words: *const i64,
    parameter_count: u64,
    parameter_words: *const i64,
    result_type_words: *const i64,
    result_count: u64,
    target: *mut u64,
    invocation_words: *mut i64,
    invocation_capacity: u64,
    invocation_len: *mut u64,
) -> i32 {
    let counts = (
        usize::try_from(parameter_count),
        usize::try_from(result_count),
        usize::try_from(invocation_capacity),
    );
    let (Ok(parameter_count), Ok(result_count), Ok(invocation_capacity)) = counts else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    let Some(parameter_type_count) = parameter_count.checked_mul(BOUNDARY_TYPE_WORDS) else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    let Some(result_type_count) = result_count.checked_mul(BOUNDARY_TYPE_WORDS) else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    if context.is_null()
        || target.is_null()
        || invocation_len.is_null()
        || !(context as *const ManagedAllocationContext).is_aligned()
        || !target.is_aligned()
        || !invocation_len.is_aligned()
        || invocation_capacity > MAX_CLOSURE_INVOCATION_WORDS
        || (parameter_count != 0 && (parameter_words.is_null() || !parameter_words.is_aligned()))
        || (parameter_type_count != 0
            && (parameter_type_words.is_null() || !parameter_type_words.is_aligned()))
        || (result_type_count != 0
            && (result_type_words.is_null() || !result_type_words.is_aligned()))
        || (invocation_capacity != 0
            && (invocation_words.is_null() || !invocation_words.is_aligned()))
    {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    }
    // SAFETY: All pointers are checked above and the generated caller retains
    // each bounded buffer for the complete synchronous callback.
    let (context, parameter_types, parameters, result_types) = unsafe {
        (
            &mut *context.cast::<ManagedAllocationContext>(),
            std::slice::from_raw_parts(parameter_type_words, parameter_type_count),
            std::slice::from_raw_parts(parameter_words, parameter_count),
            std::slice::from_raw_parts(result_type_words, result_type_count),
        )
    };
    let decode_types = |words: &[i64]| {
        words
            .chunks_exact(BOUNDARY_TYPE_WORDS)
            .map(TvmBoundaryType::from_transition_words)
            .collect::<Result<Vec<_>, _>>()
    };
    let resolved = decode_types(parameter_types)
        .and_then(|parameter_types| {
            decode_types(result_types).map(|result_types| (parameter_types, result_types))
        })
        .and_then(|(parameter_types, result_types)| {
            // SAFETY: `with_dispatch` created the runtime pointer from an
            // exclusive borrow that remains live for this callback only.
            let runtime = unsafe { &mut *context.runtime };
            let table = runtime.closure_dispatch.as_ref().cloned().ok_or_else(|| {
                "error[managed_execution.closure_resolver]: no admitted closure table".to_string()
            })?;
            let closure = usize::try_from(u64::from_ne_bytes(closure.to_ne_bytes()))
                .ok()
                .and_then(NonZeroUsize::new)
                .map(TvmRef::<ManagedClosure>::from_encoded)
                .ok_or_else(|| {
                    "error[managed_execution.closure_resolver]: invalid closure reference"
                        .to_string()
                })?;
            runtime
                .heap_ref(context.owner_id)?
                .prepare_closure_invocation(
                    closure,
                    &table,
                    table.generation(),
                    &parameter_types,
                    parameters,
                    &result_types,
                )
                .map_err(|error| format!("error[managed_execution.closure_resolver]: {error}"))
        });
    match resolved {
        Ok(invocation) if invocation.words().len() <= invocation_capacity => {
            // SAFETY: Checked non-null output buffers have sufficient capacity.
            unsafe {
                target.write(invocation.target().callable_id());
                std::ptr::copy_nonoverlapping(
                    invocation.words().as_ptr(),
                    invocation_words,
                    invocation.words().len(),
                );
                invocation_len.write(invocation.words().len() as u64);
            }
            0
        }
        Ok(_) => {
            // SAFETY: context was checked and originated in `with_dispatch`.
            let runtime = unsafe { &mut *context.runtime };
            runtime.last_allocation_error = Some(
                "error[managed_execution.closure_resolver]: invocation exceeds output capacity"
                    .to_string(),
            );
            MANAGED_ALLOCATION_FAILED_STATUS
        }
        Err(error) => {
            // SAFETY: context was checked and originated in `with_dispatch`.
            let runtime = unsafe { &mut *context.runtime };
            runtime.last_allocation_error = Some(error);
            MANAGED_ALLOCATION_FAILED_STATUS
        }
    }
}
