//! Package calls against an explicitly supplied host context.

use crate::{BoundaryError, NativeValue};

/// A package-owned operation on a host-owned context. The host must authorize,
/// schedule, and acquire that context before invocation; this binding does not
/// grant capabilities, create global state, or make blocking work scheduler-safe.
pub struct NativeContextBinding<C: ?Sized> {
    pub operation: &'static str,
    arity: usize,
    invoke: fn(&mut C, &[NativeValue]) -> Result<NativeValue, BoundaryError>,
}

impl<C: ?Sized> NativeContextBinding<C> {
    /// The callback receives exactly `arity` values and must validate their types
    /// before changing the context. It must not retain references to arguments.
    pub const fn new(
        operation: &'static str,
        arity: usize,
        invoke: fn(&mut C, &[NativeValue]) -> Result<NativeValue, BoundaryError>,
    ) -> Self {
        Self {
            operation,
            arity,
            invoke,
        }
    }

    pub fn validate_arity(&self, received: usize) -> Result<(), BoundaryError> {
        crate::native_value::validate_native_arity(self.operation, self.arity, received)
    }

    pub const fn arity(&self) -> usize {
        self.arity
    }

    /// Rejects malformed arity before calling package code or changing context.
    pub fn call(
        &self,
        context: &mut C,
        args: &[NativeValue],
    ) -> Result<NativeValue, BoundaryError> {
        self.validate_arity(args.len())?;
        (self.invoke)(context, args)
    }
}

#[cfg(test)]
#[path = "context_binding_test.rs"]
mod tests;
