//! Typed package invocation over host-owned, owner-checked resources.

use crate::{
    NativeAdapterError, NativeResourceHandle, NativeResourceOperation, NativeResourceValue,
    ResourceError, ResourceRegistry,
};

/// Storage supplied by a host, including during migration from a mixed registry.
/// Every method must check the owner, generation, and resource type before
/// returning a value. Handles are scoped to this store, not globally portable.
pub trait NativeResourceStore<R> {
    /// Borrows a live resource belonging to the caller.
    fn borrow(&self, owner: u64, handle: NativeResourceHandle) -> Result<&R, ResourceError>;
    /// Exclusively borrows a live resource belonging to the caller.
    fn borrow_mut(
        &mut self,
        owner: u64,
        handle: NativeResourceHandle,
    ) -> Result<&mut R, ResourceError>;
    /// Stores a newly produced resource under the caller's ownership.
    fn insert(&mut self, owner: u64, value: R) -> Result<NativeResourceHandle, ResourceError>;
}

impl<R> NativeResourceStore<R> for ResourceRegistry<R> {
    fn borrow(&self, owner: u64, handle: NativeResourceHandle) -> Result<&R, ResourceError> {
        self.get_for_owner(handle, owner)
    }

    fn borrow_mut(
        &mut self,
        owner: u64,
        handle: NativeResourceHandle,
    ) -> Result<&mut R, ResourceError> {
        self.get_mut_for_owner(handle, owner)
    }

    fn insert(&mut self, owner: u64, value: R) -> Result<NativeResourceHandle, ResourceError> {
        self.insert_for_owner(owner, value)
    }
}

/// Read/allocate callback; inputs borrow the actual stored resources.
pub type ResourceInvoke<R> =
    fn(&str, &[NativeResourceValue<&R>]) -> Result<NativeResourceValue<R>, NativeAdapterError>;

/// Mutable-receiver callback; arguments exclude the receiver and own snapshots.
/// The callback must validate arguments before changing the receiver on failure.
pub type ResourceMutate<R> =
    fn(&str, &mut R, Vec<NativeResourceValue<R>>) -> Result<(), NativeAdapterError>;

/// Package-owned entry points, independent of compiler/VM operation switches.
/// This descriptor grants no capability or scheduler admission. Hosts must
/// authorize and schedule the call before invoking it, and retire owner resources
/// when their actor exits. Resource outputs never enter the value-only ABI.
pub struct NativeResourceAdapter<R> {
    /// Exact source operations supported by this adapter.
    pub operations: &'static [NativeResourceOperation],
    /// Read-only or allocating operation implementation.
    pub invoke: ResourceInvoke<R>,
    /// In-place operation implementation; successful calls return the receiver.
    pub mutate: ResourceMutate<R>,
}

impl<R: Clone> NativeResourceAdapter<R> {
    /// Resolves handles only after checking the operation and arity. Reads borrow
    /// resources without cloning. Mutations snapshot other resource arguments
    /// before borrowing the receiver, including when it aliases an argument.
    pub fn call(
        &self,
        store: &mut impl NativeResourceStore<R>,
        owner: u64,
        operation: &str,
        args: &[NativeResourceValue<NativeResourceHandle>],
    ) -> Result<NativeResourceValue<NativeResourceHandle>, NativeAdapterError> {
        let contract = self
            .operations
            .iter()
            .find(|contract| contract.operation == operation)
            .ok_or_else(|| {
                NativeAdapterError::new(
                    "dispatch.unknown_operation",
                    format!("No NativeBoundary adapter is registered for `{operation}`."),
                    0,
                )
            })?;
        if args.len() != contract.arity {
            return Err(NativeAdapterError::new(
                "dispatch.arity",
                format!(
                    "operation `{operation}` expects {} arguments, got {}",
                    contract.arity,
                    args.len()
                ),
                0,
            ));
        }
        // Validate every handle before a callback or mutation snapshot can run.
        for arg in args {
            if let NativeResourceValue::Resource(handle) = arg {
                store.borrow(owner, *handle)?;
            }
        }
        if contract.mutates_receiver {
            if !contract.returns_resource {
                return Err(invalid_return_contract(operation));
            }
            let Some(NativeResourceValue::Resource(receiver)) = args.first() else {
                return Err(NativeAdapterError::new(
                    "dispatch.type",
                    format!("operation `{operation}` argument 0 must be a resource handle"),
                    0,
                ));
            };
            let values = args[1..]
                .iter()
                .map(|arg| match arg {
                    NativeResourceValue::Value(value) => {
                        Ok(NativeResourceValue::Value(value.clone()))
                    }
                    NativeResourceValue::Resource(handle) => store
                        .borrow(owner, *handle)
                        .cloned()
                        .map(NativeResourceValue::Resource),
                })
                .collect::<Result<Vec<_>, ResourceError>>()?;
            (self.mutate)(operation, store.borrow_mut(owner, *receiver)?, values)?;
            return Ok(NativeResourceValue::Resource(*receiver));
        }
        let values = args
            .iter()
            .map(|arg| match arg {
                NativeResourceValue::Value(value) => Ok(NativeResourceValue::Value(value.clone())),
                NativeResourceValue::Resource(handle) => store
                    .borrow(owner, *handle)
                    .map(NativeResourceValue::Resource),
            })
            .collect::<Result<Vec<_>, ResourceError>>()?;
        let result = (self.invoke)(operation, &values)?;
        if matches!(result, NativeResourceValue::Resource(_)) != contract.returns_resource {
            return Err(invalid_return_contract(operation));
        }
        match result {
            NativeResourceValue::Value(value) => Ok(NativeResourceValue::Value(value)),
            NativeResourceValue::Resource(value) => store
                .insert(owner, value)
                .map(NativeResourceValue::Resource)
                .map_err(Into::into),
        }
    }
}

fn invalid_return_contract(operation: &str) -> NativeAdapterError {
    NativeAdapterError::new(
        "dispatch.contract",
        format!("operation `{operation}` violates its declared resource return contract"),
        0,
    )
}

impl From<ResourceError> for NativeAdapterError {
    fn from(error: ResourceError) -> Self {
        Self::new(error.code(), error.message(), 0)
    }
}

#[cfg(test)]
#[path = "resource_adapter_test.rs"]
mod tests;
