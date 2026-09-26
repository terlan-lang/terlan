use super::*;

/// Validates explicit mutable borrows on non-receiver opaque resources.
pub(super) fn validate_argument_mutability(
    manifest: &CAbiBindingManifest,
    function: &CAbiBindingFunction,
) -> Result<(), String> {
    for (index, argument) in function.args.iter().enumerate() {
        if !argument.mutable {
            continue;
        }
        if function.role != CAbiFunctionRole::MutableMethod
            || index == 0
            || binding_type(manifest, &argument.ty).is_none()
        {
            return Err(format!(
                "error[native_bindgen.mutable_argument_contract]: `{}.{}` may be mutable only when it is a non-receiver opaque-resource argument of a mutable method",
                function.name, argument.name
            ));
        }
    }
    Ok(())
}

/// Validates the aliases returned for all caller-owned output resources.
pub(super) fn validate_discarded_dispatcher_output_cardinality(
    function: &CAbiBindingFunction,
    dispatcher: &CDispatcherBinding,
) -> Result<(), String> {
    let discards = matches!(
        dispatcher.output,
        CDispatcherOutput::DiscardOwnedHandle { .. }
            | CDispatcherOutput::DiscardOwnedHandleTuple { .. }
    );
    if discards
        && dispatcher.output.indices().len()
            != 1 + function
                .args
                .iter()
                .filter(|argument| argument.mutable)
                .count()
    {
        return Err(format!(
            "error[native_bindgen.c_dispatcher_contract]: dispatcher binding `{}` must discard exactly one returned owned alias for the receiver and each explicitly mutable output argument",
            function.name
        ));
    }
    Ok(())
}
