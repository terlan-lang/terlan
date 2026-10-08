//! Admitted source callbacks at the serving boundary.

use crate::runtime::vm::native_callable::VmNativeCallableRef;
use crate::runtime::vm::{ReplValue, VmRuntimeResult};
use terlan_http_native::routing::Router;

use super::invocation::AotHandlerInvocationStep;
use super::{finish_immediate_step, AotHandlerRuntime};

impl AotHandlerRuntime {
    pub(in crate::commands::serve) fn begin_callable_invocation(
        &self,
        module: &str,
        callable: &ReplValue,
        args: Vec<ReplValue>,
    ) -> VmRuntimeResult<AotHandlerInvocationStep> {
        match self.validate_callable(module, callable, args.len())? {
            Some(named) => self
                .begin_request_invocation(module, &named.function, args)
                .map_err(Into::into),
            None => self.begin_closure_invocation(callable.clone(), args),
        }
    }

    pub(in crate::commands::serve) fn execute_callable(
        &self,
        module: &str,
        callable: &ReplValue,
        args: Vec<ReplValue>,
        output: &mut dyn FnMut(&str),
    ) -> VmRuntimeResult<ReplValue> {
        let Some(callable) = self.validate_callable(module, callable, args.len())? else {
            return finish_immediate_step(self.begin_closure_invocation(callable.clone(), args)?)
                .map_err(Into::into);
        };
        self.execute_immediate_native(module, &callable.function, args, output)
            .map_err(|error| {
                format!(
                    "error[serve.aot.callable]: callback `{}.{}/{}` failed: {error}",
                    callable.module, callable.function, callable.arity
                )
                .into()
            })
    }

    pub(super) fn validate_callable(
        &self,
        module: &str,
        callable: &ReplValue,
        arity: usize,
    ) -> VmRuntimeResult<Option<VmNativeCallableRef>> {
        if module != self.module {
            return Err(
                "error[serve.aot.callable]: callback belongs to a different module image".into(),
            );
        }
        if let ReplValue::Closure(value) = callable {
            if value.descriptor.parameters().len() != arity {
                return Err("error[serve.aot.callable]: callback argument arity mismatch".into());
            }
            return Ok(None);
        }
        let named = VmNativeCallableRef::from_value(callable)
            .ok_or("error[serve.aot.callable]: router value is not an admitted native callback")?;
        if named.module != module || named.arity != arity {
            return Err(format!(
                "error[serve.aot.callable]: callback `{}.{}/{}` cannot be invoked as `{module}` with {arity} arguments",
                named.module, named.function, named.arity
            ).into());
        }
        Ok(Some(named))
    }

    pub(in crate::commands::serve) fn callable_arity(&self, callable: &ReplValue) -> Option<usize> {
        if let ReplValue::Closure(value) = callable {
            return Some(value.descriptor.parameters().len());
        }
        VmNativeCallableRef::from_value(callable).map(|callable| callable.arity)
    }

    pub(in crate::commands::serve) fn execute_http_router(
        &self,
        module: &str,
        function: &str,
        _output: &mut dyn FnMut(&str),
    ) -> VmRuntimeResult<Router<ReplValue>> {
        if module != self.module || function != "router" {
            return Err(format!(
                "error[serve.aot.router]: native router `{module}.{function}/0` is not loaded"
            )
            .into());
        }
        self.router.clone().ok_or_else(|| {
            format!("error[serve.aot.router]: module `{module}` has no admitted source router")
                .into()
        })
    }
}
