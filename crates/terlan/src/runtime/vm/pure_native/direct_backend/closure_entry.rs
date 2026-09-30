//! Owned closures enter the same image dispatch and continuation path as exports.

use crate::runtime::vm::{ReplValue, VmRuntimeResult};

use super::{
    encode_public_argument, DirectNativeBackend, PureNativeExecutionContext, TvmBoundaryType,
    TvmControlFrame,
};

impl DirectNativeBackend {
    pub(super) fn dispatch_closure(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        closure: &ReplValue,
        args: &[ReplValue],
    ) -> VmRuntimeResult<TvmControlFrame> {
        let ReplValue::Closure(value) = closure else {
            return Err("error[execution_shard.closure]: expected an owned function value".into());
        };
        let table = &self.image.closures;
        let callable = table
            .validate_descriptor(&value.descriptor, value.captures.len())
            .map_err(|error| format!("error[execution_shard.closure]: {error}"))?;
        if callable.parameters.len() != args.len() {
            return Err("error[execution_shard.arity]: closure argument count differs".into());
        }
        let owner = context.owner_id();
        let semantic = value.descriptor.semantic_id();
        let word = encode_public_argument(
            context.managed(),
            owner,
            &TvmBoundaryType::Managed(semantic.bytes()),
            closure,
        )?;
        let arguments = callable
            .parameters
            .iter()
            .zip(args)
            .map(|(ty, value)| encode_public_argument(context.managed(), owner, ty, value))
            .collect::<Result<Vec<_>, _>>()?;
        let invocation = context
            .managed_ref()
            .with_public_materialization(owner, |heap, _| {
                let reference = heap
                    .validate_abi_reference(u64::from_ne_bytes(word.to_ne_bytes()), semantic)
                    .map_err(|error| format!("error[execution_shard.closure]: {error}"))?;
                heap.prepare_closure_invocation(
                    reference.cast(),
                    table,
                    table.generation(),
                    &callable.parameters,
                    &arguments,
                    &callable.results,
                )
                .map_err(|error| format!("error[execution_shard.closure]: {error}"))
            })?;
        self.dispatch(
            context,
            request_id,
            invocation.target().callable_id(),
            invocation.words(),
            Vec::new(),
        )
        .map_err(Into::into)
    }
}
