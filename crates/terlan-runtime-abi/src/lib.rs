#![forbid(unsafe_code)]

//! Stable types shared by Terlan AOT images, the VM, and capability workers.

mod actor_state;
mod adapter_error;
mod boundary_error;
mod boundary_type;
mod callback_invocation;
mod context_binding;
mod descriptor_value;
mod native_argument;
mod native_record;
mod native_services;
mod native_value;
mod resource;
mod resource_adapter;
mod resource_operation;

pub use actor_state::ActorStateStore;
pub use adapter_error::NativeAdapterError;
pub use boundary_error::{BoundaryError, ErrorDomain};
pub use boundary_type::TvmBoundaryType;
pub use callback_invocation::{CallbackInvocation, CallbackState};
pub use context_binding::NativeContextBinding;
pub use descriptor_value::{DescriptorValue, DescriptorView, OwnedDescriptor};
pub use native_argument::FromNativeValue;
pub use native_services::NativeServices;
pub use native_value::{NativeBinding, NativeValue};
pub use resource::{NativeResourceHandle, ResourceError, ResourceRegistry, SYSTEM_RESOURCE_OWNER};
pub use resource_adapter::{
    NativeResourceAdapter, NativeResourceStore, ResourceInvoke, ResourceMutate,
};
pub use resource_operation::{NativeResourceOperation, NativeResourceValue};
#[cfg(feature = "poll-io")]
pub mod poll_io;
