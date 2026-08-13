//! Safe managed-heap embedding used by runtime benchmarks.

pub(crate) use crate::runtime::native_image::managed::{
    ActorHeap, ActorId, AllocationClass, HeapLimits, ManagedRoot, ManagedTypeDescriptor,
    RootLocation, SemanticTypeId,
};
