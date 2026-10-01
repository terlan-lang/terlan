//! Explicit application-scoped native contexts shared across execution shards.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::{BoundaryError, ErrorDomain, NativeContextBinding, NativeValue};

type ServiceCall = dyn Fn(&[NativeValue]) -> Result<NativeValue, BoundaryError> + Send + Sync;

#[derive(Clone)]
struct ServiceBinding {
    arity: usize,
    invoke: Arc<ServiceCall>,
}

/// Host-granted operations, not a global registry. Clones preserve the exact
/// shared contexts but later registrations do not expand existing clones.
/// Calls serialize access to each context; the host remains responsible for
/// scheduling potentially blocking work outside actor execution.
#[derive(Clone, Default)]
pub struct NativeServices {
    bindings: BTreeMap<&'static str, ServiceBinding>,
}

impl fmt::Debug for NativeServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeServices")
            .field("operations", &self.bindings.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl NativeServices {
    /// Tests an exact application grant without acquiring its context.
    pub fn contains(&self, operation: &str) -> bool {
        self.bindings.contains_key(operation)
    }

    /// Registers a package's exact operations atomically. Duplicate grants never
    /// replace authority or leave part of a batch installed.
    pub fn register_context<C: Send + 'static>(
        &mut self,
        context: Arc<Mutex<C>>,
        bindings: impl IntoIterator<Item = NativeContextBinding<C>>,
    ) -> Result<(), BoundaryError> {
        let mut pending = BTreeMap::new();
        for binding in bindings {
            let operation = binding.operation;
            if operation.is_empty() {
                return Err(service_error(
                    "registration",
                    "operation name cannot be empty",
                ));
            }
            if self.bindings.contains_key(operation) || pending.contains_key(operation) {
                return Err(service_error(
                    "registration",
                    format!("operation `{operation}` is already registered"),
                ));
            }
            let context = Arc::clone(&context);
            let arity = binding.arity();
            let invoke = Arc::new(move |args: &[NativeValue]| {
                let mut context = context.lock().map_err(|_| {
                    service_error("poisoned", format!("context for `{operation}` is poisoned"))
                })?;
                binding.call(&mut context, args)
            });
            pending.insert(operation, ServiceBinding { arity, invoke });
        }
        self.bindings.extend(pending);
        Ok(())
    }

    /// Checks authority and arity without acquiring a context or executing code.
    pub fn validate_arity(&self, operation: &str, received: usize) -> Result<(), BoundaryError> {
        let binding = self.resolve(operation)?;
        crate::native_value::validate_native_arity(operation, binding.arity, received)
    }

    pub fn call(
        &self,
        operation: &str,
        args: &[NativeValue],
    ) -> Result<NativeValue, BoundaryError> {
        let binding = self.resolve(operation)?;
        crate::native_value::validate_native_arity(operation, binding.arity, args.len())?;
        (binding.invoke)(args)
    }

    fn resolve(&self, operation: &str) -> Result<&ServiceBinding, BoundaryError> {
        self.bindings.get(operation).ok_or_else(|| {
            service_error(
                "unavailable",
                format!("operation `{operation}` has no granted context"),
            )
        })
    }
}

fn service_error(kind: &str, message: impl fmt::Display) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "native service context",
        format!("error[native_service.{kind}]: {message}"),
    )
}

#[cfg(test)]
#[path = "native_services_test.rs"]
mod tests;
