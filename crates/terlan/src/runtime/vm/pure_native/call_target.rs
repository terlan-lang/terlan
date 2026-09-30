//! Named exports and owned closures share actor entry, tracing, and suspension.

use crate::runtime::vm::{ReplValue, VmRuntimeResult};
use std::borrow::Cow;

use super::{PreparedNativeCall, PureNativeBoundary, PureNativeExecutionContext, VmProcessSource};

#[derive(Clone, Debug)]
pub(crate) enum NativeCallTarget<'a> {
    Export(Cow<'a, str>),
    Closure(Cow<'a, ReplValue>),
}

impl From<String> for NativeCallTarget<'static> {
    fn from(value: String) -> Self {
        Self::Export(value.into())
    }
}

impl NativeCallTarget<'_> {
    pub(super) fn source_name(&self) -> String {
        match self {
            Self::Export(name) => name.to_string(),
            Self::Closure(value) => match value.as_ref() {
                ReplValue::Closure(value) => format!("closure_{}", value.descriptor.callable_id()),
                _ => "invalid_closure".into(),
            },
        }
    }
}

impl PureNativeBoundary {
    pub(super) fn prepare_target_call(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        target: &NativeCallTarget<'_>,
        args: &[ReplValue],
        trace_enabled: bool,
    ) -> VmRuntimeResult<PreparedNativeCall> {
        let value = match target {
            NativeCallTarget::Export(function) => {
                return self
                    .prepare_call(context, function, args, trace_enabled)
                    .map_err(Into::into);
            }
            NativeCallTarget::Closure(value) => value.as_ref(),
        };
        let ReplValue::Closure(value) = value else {
            return Err("error[execution_shard.closure]: expected an owned function value".into());
        };
        let table = context
            .managed_ref()
            .admitted_closures()
            .ok_or("error[execution_shard.closure]: no admitted executable generation")?;
        let callable = table
            .validate_descriptor(&value.descriptor, value.captures.len())
            .map_err(|error| format!("error[execution_shard.closure]: {error}"))?;
        if callable.parameters.len() != args.len() {
            return Err(format!(
                "error[execution_shard.arity]: closure expects {} arguments, received {}",
                callable.parameters.len(),
                args.len()
            )
            .into());
        }
        let [result] = callable.results.as_slice() else {
            return Err("error[execution_shard.closure]: function must return one value".into());
        };
        if self.artifact.is_none() || self.backend.is_none() {
            return Err("error[execution_shard.closure]: no admitted backend".into());
        }
        Ok(PreparedNativeCall {
            request_id: context.allocate_request_id()?,
            owner_id: context.owner_id(),
            export_id: callable.id,
            result_type: result.clone(),
            continuations: None,
            trace_source: trace_enabled
                .then(|| VmProcessSource::new("native", target.source_name(), args.len())),
        })
    }
}
