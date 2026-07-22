//! Canonical TVM native executable-image admission.

mod boundary_type;
pub mod control;
pub(crate) mod debug;
mod descriptor;
mod image;
pub mod managed;
pub mod package_validation;
mod sealed;

pub use boundary_type::TvmBoundaryType;
pub use descriptor::{
    decode_descriptor, encode_descriptor, TvmCallableDescriptor, TvmContinuationDescriptor,
    TvmDependencyDescriptor, TvmExecutableDescriptor, TvmExportDescriptor, TvmImageIdentity,
    TvmImageIntegrity, TvmImageTarget, TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor,
    TvmNativeResourceDescriptor, TvmSignatureDescriptor, TVM_DISPATCH_SYMBOL_V2,
    TVM_IMAGE_ENTRY_SYMBOL_V1,
};
pub use image::{
    descriptor_object_for_native, descriptor_object_for_native_with_debug, host_tvm_target,
    inspect_tvm_image, seal_tvm_image, TvmNativeImageInspection,
};
pub(crate) use sealed::{reject_tvm_image_sidecars, SealedTvmImage};

/// Maximum transition words forwarded by one image-local indirect invocation.
///
/// This bound is shared by generated code and the execution shard so a closure
/// can call any admitted suspending target without retaining a native stack.
pub(crate) const TVM_INDIRECT_TRANSITION_WORD_CAPACITY: usize = 128;

#[cfg(test)]
#[path = "native_image_test.rs"]
mod native_image_test;
