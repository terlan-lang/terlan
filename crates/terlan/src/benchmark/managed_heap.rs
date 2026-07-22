//! Safe managed-heap embedding used by runtime benchmarks.

#![allow(dead_code)]

#[path = "../runtime/native_image/managed/core.rs"]
mod core;
#[path = "../runtime/native_image/managed/heap.rs"]
mod heap;
#[path = "../runtime/native_image/managed/layout.rs"]
mod layout;
#[path = "../runtime/native_image/managed/mailbox.rs"]
mod mailbox;
#[path = "../runtime/native_image/managed/roots.rs"]
mod roots;

pub(crate) use core::{ActorId, ManagedMemoryError, TvmRef};
pub(crate) use heap::{ActorHeap, HeapLimits};
pub(crate) use layout::{AllocationClass, ManagedTypeDescriptor, SemanticTypeId};
pub(crate) use mailbox::ManagedMailboxFragment;
pub(crate) use roots::{ManagedRoot, RootLocation};
