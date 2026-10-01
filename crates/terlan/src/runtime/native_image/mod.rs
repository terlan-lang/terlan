//! Canonical TVM native executable-image admission.

mod boundary_type;
pub mod control;
pub(crate) mod debug;
mod descriptor;
pub(crate) mod dispatch_lookup;
mod image;
pub mod managed;
pub mod package_validation;
mod sealed;

pub use boundary_type::TvmBoundaryType;
pub use descriptor::{
    decode_descriptor, encode_descriptor, TvmCallableDescriptor, TvmContinuationDescriptor,
    TvmDependencyDescriptor, TvmExecutableDescriptor, TvmExportDescriptor, TvmImageIdentity,
    TvmImageIntegrity, TvmImageTarget, TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor,
    TvmNativeResourceDescriptor, TvmSignatureDescriptor, TVM_DISPATCH_SYMBOL_V4,
    TVM_IMAGE_ENTRY_SYMBOL_V1,
};
pub use image::{
    descriptor_object_for_native, descriptor_object_for_native_with_debug, host_tvm_target,
    inspect_tvm_image, seal_tvm_image, TvmNativeImageInspection,
};
pub(crate) use sealed::{reject_tvm_image_sidecars, SealedTvmImage};

/// Maximum transition words forwarded by one image-local indirect invocation.
///
/// Indirect targets can themselves call suspending functions. They use the
/// same caller-owned completion storage as direct calls, not a smaller frame.
pub(crate) const TVM_INDIRECT_TRANSITION_WORD_CAPACITY: usize =
    TVM_COMPLETION_TRANSITION_WORD_CAPACITY;

/// Maximum words retained by a recursively composed native completion stack.
///
/// ABI-1 keeps transition storage caller-owned and contiguous. Recursive
/// non-tail suspension therefore uses this deterministic resource bound rather
/// than requiring an impossible compile-time proof of an unbounded width.
pub(crate) const TVM_COMPLETION_TRANSITION_WORD_CAPACITY: usize = 8_192;
/// Transition scratch shared by loaded images and linked execution probes.
/// Reserve the indirect frame plus capability arguments and completion words.
pub(crate) fn transition_scratch_capacity(callable_width: usize) -> usize {
    TVM_INDIRECT_TRANSITION_WORD_CAPACITY
        .saturating_add(callable_width.saturating_mul(5).saturating_add(6))
        .max(TVM_COMPLETION_TRANSITION_WORD_CAPACITY)
}

#[cfg(test)]
#[path = "native_image_test.rs"]
#[cfg(test)]
mod native_image_test;
