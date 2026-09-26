//! Owner-scoped managed execution context for generated native calls.

use std::collections::{BTreeSet, HashMap};
use std::ffi::c_void;
use std::num::NonZeroUsize;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;

use crate::runtime::native_image::{
    TvmBoundaryType, TvmCallableDescriptor, TvmManagedCollectionDescriptor,
    TvmManagedLayoutDescriptor,
};
use crate::runtime::vm::http_session::VmHttpSessionService;

use super::ManagedRoot;
use super::{
    ActorHeap, ActorId, AtomIndex, HeapLimits, ManagedBinary, ManagedBytes, ManagedClosure,
    ManagedClosureDispatchTable, ManagedClosureImageGeneration, ManagedContinuation,
    ManagedLayoutRegistry, ManagedMailboxFragment, ManagedString, TvmRef,
    MANAGED_ALLOCATION_FAILED_STATUS, MAX_MANAGED_AGGREGATE_ABI_BYTES,
};

#[path = "execution/actor_transfer.rs"]
mod actor_transfer;
pub(crate) use actor_transfer::ManagedActorTransfer;
#[path = "execution/abi_types.rs"]
mod abi_types;
use abi_types::{
    managed_semantic_id, reference_word, ManagedAllocator, ManagedCallableRecorder,
    ManagedClosureResolver,
};
#[path = "execution/hibernation.rs"]
mod hibernation;
#[path = "execution/owner_heaps.rs"]
mod owner_heaps;
use owner_heaps::ManagedOwnerHeaps;

const DEFAULT_SOFT_HEAP_BYTES: usize = 1024 * 1024;
const DEFAULT_HARD_HEAP_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MAILBOX_TRANSFER_WORK_BYTES: usize = 64 * 1024 * 1024;
const MAX_AGGREGATE_FIELD_WORDS: usize = MAX_MANAGED_AGGREGATE_ABI_BYTES / 2;
const MAX_RECYCLED_ACTOR_HEAPS: usize = 4;
const MAX_RECYCLED_ACTOR_HEAP_BYTES: usize = 256 * 1024;

const BOUNDARY_TYPE_WORDS: usize = 3;
const MAX_CLOSURE_INVOCATION_WORDS: usize = 128;

/// Lazily materialized actor heaps retained by one execution shard.
#[derive(Debug)]
pub(crate) struct ManagedExecutionRuntime {
    /// Default limits copied into every lazily created actor heap.
    limits: HeapLimits,
    /// Immutable managed layouts and collection schemas admitted with the image.
    layouts: Arc<ManagedLayoutRegistry>,
    /// Authenticated closure-call membership for the admitted image generation.
    closure_dispatch: Option<Arc<ManagedClosureDispatchTable>>,
    /// Actor heaps exclusively owned by this execution-runtime instance.
    heaps: ManagedOwnerHeaps,
    /// Reclaimed semispaces available only to this fixed execution shard.
    recycled_heaps: Vec<ActorHeap>,
    /// Precise mailbox roots retained while VM messages carry opaque tokens.
    mailbox_fragments: HashMap<u32, ManagedMailboxFragment>,
    /// Next nonzero shard-local managed mailbox fragment identity.
    next_mailbox_fragment_id: u32,
    /// Shared VM-owned HTTP session actors available to request shards.
    http_sessions: Option<VmHttpSessionService>,
    /// Last synchronous allocator diagnostic retained across the C ABI return.
    last_allocation_error: Option<String>,
    /// Whether generated callable-entry probes should retain stable identities.
    callable_coverage_enabled: bool,
    /// Stable callable identities observed while coverage collection is active.
    covered_callables: BTreeSet<u64>,
    /// Optional append-only cross-process coverage stream selected by the caller.
    callable_coverage_file: Option<PathBuf>,
}

/// Byte offset read by generated backedges to observe actor-heap pressure.
/// This is part of the in-process AOT/runtime ABI. Keep the offset assertion
/// beside the C-layout context so a field reorder fails at compile time.
pub(crate) const MANAGED_CONTEXT_COLLECTION_REQUESTED_OFFSET: i32 = 0;

/// Stack-bound context passed only for the duration of one generated dispatch.
#[repr(C)]
struct ManagedAllocationContext {
    /// Set after allocation crosses the adaptive threshold. Generated code
    /// observes this at its next recursive safepoint and yields with precise
    /// continuation roots before the hard heap limit is approached.
    collection_requested: u64,
    /// Exclusively borrowed runtime that owns the destination actor heap.
    runtime: *mut ManagedExecutionRuntime,
    /// Nonzero actor identity selected for this synchronous dispatch.
    owner_id: u64,
}

const _: () = assert!(
    std::mem::offset_of!(ManagedAllocationContext, collection_requested)
        == MANAGED_CONTEXT_COLLECTION_REQUESTED_OFFSET as usize
);

/// Owner-local managed roots withheld from the external continuation protocol.
#[derive(Debug)]
pub(crate) struct PendingManagedCaptures {
    /// Actor that exclusively owns every retained root.
    owner: ActorId,
    /// Precise roots tied to the generated continuation identity.
    continuation: ManagedContinuation,
    /// Generated parameter positions occupied by managed roots.
    positions: Box<[usize]>,
    /// Complete generated capture count before scalar projection.
    capture_count: usize,
}

impl PendingManagedCaptures {
    /// Returns the actor that owns every precise continuation root.
    pub(crate) fn owner_id(&self) -> u64 {
        self.owner.get()
    }
}

impl ManagedExecutionRuntime {
    /// Borrows the immutable image layouts for admission-time runtime projections.
    pub(crate) fn layout_registry(&self) -> &ManagedLayoutRegistry {
        &self.layouts
    }

    /// Creates an execution runtime with the default per-actor heap limits.
    pub(crate) fn runtime_default() -> Result<Self, String> {
        let limits = HeapLimits::new(DEFAULT_SOFT_HEAP_BYTES, DEFAULT_HARD_HEAP_BYTES)
            .map_err(|error| format!("error[managed_execution.limits]: {error}"))?;
        let callable_coverage_file = std::env::var_os("TERLAN_CALLABLE_COVERAGE_FILE")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        Ok(Self {
            limits,
            layouts: Arc::new(ManagedLayoutRegistry::default()),
            closure_dispatch: None,
            heaps: ManagedOwnerHeaps::default(),
            recycled_heaps: Vec::new(),
            mailbox_fragments: HashMap::new(),
            next_mailbox_fragment_id: 0,
            http_sessions: None,
            last_allocation_error: None,
            callable_coverage_enabled: callable_coverage_file.is_some(),
            covered_callables: BTreeSet::new(),
            callable_coverage_file,
        })
    }

    /// Creates an execution runtime from the aggregate layouts admitted with an image.
    #[cfg(test)]
    pub(crate) fn with_image_layouts(
        layouts: &[TvmManagedLayoutDescriptor],
    ) -> Result<Self, String> {
        Self::with_image_metadata(layouts, &[], &[])
    }

    /// Creates an execution runtime from all managed metadata admitted with an image.
    pub(crate) fn with_image_metadata(
        layouts: &[TvmManagedLayoutDescriptor],
        collections: &[TvmManagedCollectionDescriptor],
        atoms: &[String],
    ) -> Result<Self, String> {
        let mut runtime = Self::runtime_default()?;
        runtime.layouts = Arc::new(ManagedLayoutRegistry::from_image(
            layouts,
            collections,
            atoms,
        )?);
        Ok(runtime)
    }

    /// Creates an execution runtime with authenticated image-local closure dispatch.
    pub(crate) fn with_executable_image_metadata(
        layouts: &[TvmManagedLayoutDescriptor],
        collections: &[TvmManagedCollectionDescriptor],
        atoms: &[String],
        descriptor_digest: [u8; 32],
        callables: &[TvmCallableDescriptor],
    ) -> Result<Self, String> {
        let mut runtime = Self::with_image_metadata(layouts, collections, atoms)?;
        let generation = ManagedClosureImageGeneration::new(descriptor_digest)
            .map_err(|error| format!("error[managed_execution.closure_generation]: {error}"))?;
        runtime.closure_dispatch = Some(Arc::new(
            ManagedClosureDispatchTable::admit(generation, callables)
                .map_err(|error| format!("error[managed_execution.closure_dispatch]: {error}"))?,
        ));
        Ok(runtime)
    }

    /// Returns the authenticated callable membership of this executable generation.
    #[cfg(test)]
    pub(crate) fn closure_dispatch(&self) -> Result<&ManagedClosureDispatchTable, String> {
        self.closure_dispatch.as_deref().ok_or_else(|| {
            "error[managed_execution.closure_dispatch]: runtime has no admitted executable generation"
                .to_string()
        })
    }

    /// Encodes compiler-known public atom text as one image-local native word.
    pub(crate) fn encode_atom_value(&self, value: &str) -> Result<i64, String> {
        self.layouts
            .atom_index(value)
            .map(|index| i64::from(index.get()))
            .map_err(|error| format!("error[managed_execution.atom]: {error}"))
    }

    /// Materializes one image-local native atom word as canonical public text.
    pub(crate) fn materialize_atom_value(&self, value: i64) -> Result<String, String> {
        let index = u32::try_from(value)
            .map(AtomIndex::from_runtime)
            .map_err(|_| "error[managed_execution.atom]: invalid atom index".to_string())?;
        self.layouts
            .atom_identity(index)
            .map(str::to_owned)
            .map_err(|error| format!("error[managed_execution.atom]: {error}"))
    }

    /// Creates an empty heap set that shares this runtime's immutable image layouts.
    pub(crate) fn fork_empty(&self) -> Self {
        Self {
            limits: self.limits,
            layouts: Arc::clone(&self.layouts),
            closure_dispatch: self.closure_dispatch.as_ref().map(Arc::clone),
            heaps: ManagedOwnerHeaps::default(),
            recycled_heaps: Vec::new(),
            mailbox_fragments: HashMap::new(),
            next_mailbox_fragment_id: 0,
            http_sessions: self.http_sessions.clone(),
            last_allocation_error: None,
            callable_coverage_enabled: self.callable_coverage_file.is_some(),
            covered_callables: BTreeSet::new(),
            callable_coverage_file: self.callable_coverage_file.clone(),
        }
    }

    /// Starts a fresh callable-coverage interval for this execution shard.
    pub(crate) fn start_callable_coverage(&mut self) {
        self.covered_callables.clear();
        self.callable_coverage_enabled = true;
    }

    /// Stops coverage collection and returns the exact executed callable identities.
    pub(crate) fn finish_callable_coverage(&mut self) -> BTreeSet<u64> {
        self.callable_coverage_enabled = false;
        std::mem::take(&mut self.covered_callables)
    }

    /// Returns the optional callback passed into generated native entries.
    pub(crate) fn callable_coverage_callback(&self) -> *const c_void {
        if self.callable_coverage_enabled {
            callbacks::managed_record_callable as ManagedCallableRecorder as *const c_void
        } else {
            std::ptr::null()
        }
    }

    /// Attaches one VM-owned HTTP session runtime to this image template.
    pub(crate) fn attach_http_sessions(&mut self, sessions: VmHttpSessionService) {
        self.http_sessions = Some(sessions);
    }

    /// Takes the exact managed allocator diagnostic from the last dispatch.
    pub(crate) fn take_allocation_error(&mut self) -> Option<String> {
        self.last_allocation_error.take()
    }

    /// Retains the first generated-code failure until control returns across
    /// the native ABI. Later callbacks must not hide its root cause.
    pub(super) fn retain_allocation_error(&mut self, error: String) {
        self.last_allocation_error.get_or_insert(error);
    }

    /// Copies one generated managed word directly into receiver-owned mailbox storage.
    pub(crate) fn copy_mailbox_value(
        &mut self,
        sender_id: u64,
        receiver_id: u64,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ManagedMailboxFragment, String> {
        let semantic_id = managed_semantic_id(boundary_type)?.ok_or_else(|| {
            "error[managed_execution.mailbox_type]: mailbox graph type is not managed".to_string()
        })?;
        let root = self.boundary_reference(sender_id, boundary_type, value)?;
        self.next_mailbox_fragment_id = self
            .next_mailbox_fragment_id
            .checked_add(1)
            .filter(|identity| *identity != 0)
            .ok_or_else(|| {
                "error[managed_execution.mailbox_identity]: mailbox fragment identities exhausted"
                    .to_string()
            })?;
        let fragment_id = self.next_mailbox_fragment_id;
        let fragment = if sender_id == receiver_id {
            self.heap_ref(sender_id)?
                .retain_message_graph(root, semantic_id, fragment_id)
                .map_err(|error| format!("error[managed_execution.mailbox_copy]: {error}"))?
        } else {
            self.heap(receiver_id)?;
            let mut receiver = self.heaps.remove(&receiver_id).ok_or_else(|| {
                format!(
                    "error[managed_execution.mailbox_copy]: receiver {receiver_id} heap disappeared"
                )
            })?;
            let copied = self
                .heap_ref(sender_id)?
                .copy_message_graph_to(
                    root,
                    semantic_id,
                    &mut receiver,
                    fragment_id,
                    DEFAULT_MAILBOX_TRANSFER_WORK_BYTES,
                )
                .map_err(|error| format!("error[managed_execution.mailbox_copy]: {error}"));
            self.heaps.insert(receiver_id, receiver);
            copied?
        };
        self.mailbox_fragments.insert(fragment_id, fragment.clone());
        Ok(fragment)
    }

    /// Removes a just-copied graph when VM mailbox admission rejects publication.
    pub(crate) fn rollback_mailbox_value(&mut self, fragment_id: u32) -> Result<(), String> {
        let fragment = self
            .mailbox_fragments
            .get(&fragment_id)
            .cloned()
            .ok_or_else(|| {
            format!(
                "error[managed_execution.mailbox_rollback]: fragment {fragment_id} is not registered"
            )
        })?;
        self.heaps
            .get_mut(&fragment.receiver().get())
            .ok_or_else(|| {
                format!(
                    "error[managed_execution.mailbox_rollback]: receiver {} heap is missing",
                    fragment.receiver().get()
                )
            })?
            .rollback_message_graph(&fragment)
            .map_err(|error| format!("error[managed_execution.mailbox_rollback]: {error}"))?;
        self.mailbox_fragments.remove(&fragment_id);
        Ok(())
    }

    /// Resolves one queued receiver-owned graph into its validated native word.
    pub(crate) fn mailbox_value_word(
        &self,
        fragment_id: u32,
        receiver_id: u64,
        boundary_type: &TvmBoundaryType,
    ) -> Result<i64, String> {
        let fragment = self.mailbox_fragments.get(&fragment_id).ok_or_else(|| {
            format!("error[managed_execution.mailbox_stale]: fragment {fragment_id} is missing")
        })?;
        if fragment.receiver().get() != receiver_id {
            return Err(
                "error[managed_execution.mailbox_owner]: mailbox fragment receiver mismatch"
                    .to_string(),
            );
        }
        let word = reference_word(fragment.root_reference());
        self.validate_boundary_reference(receiver_id, boundary_type, word)?;
        Ok(word)
    }

    /// Releases one precise mailbox root after its message is consumed.
    pub(crate) fn consume_mailbox_value(&mut self, fragment_id: u32) -> Result<(), String> {
        self.mailbox_fragments
            .remove(&fragment_id)
            .map(|_| ())
            .ok_or_else(|| {
                format!("error[managed_execution.mailbox_stale]: fragment {fragment_id} is missing")
            })
    }

    /// Releases one actor heap after its boundary and continuations have shut down.
    pub(crate) fn release_owner(&mut self, owner_id: u64) {
        if let Some(mut heap) = self.heaps.remove(&owner_id) {
            heap.reclaim_for_pool();
            let retained = heap.retained_capacity_bytes();
            let pooled = self
                .recycled_heaps
                .iter()
                .map(ActorHeap::retained_capacity_bytes)
                .sum::<usize>();
            if self.recycled_heaps.len() < MAX_RECYCLED_ACTOR_HEAPS
                && pooled.saturating_add(retained) <= MAX_RECYCLED_ACTOR_HEAP_BYTES
            {
                self.recycled_heaps.push(heap);
            }
        }
        self.mailbox_fragments
            .retain(|_, fragment| fragment.receiver().get() != owner_id);
    }

    /// Reclaims request-local objects while retaining a live fixed owner's heap.
    pub(crate) fn reset_owner(&mut self, owner_id: u64) {
        if let Some(heap) = self.heaps.get_mut(&owner_id) {
            heap.reclaim_for_reuse();
        }
        self.mailbox_fragments
            .retain(|_, fragment| fragment.receiver().get() != owner_id);
    }

    /// Runs one compound public allocation atomically in an actor-local heap.
    pub(crate) fn with_public_allocation<R>(
        &mut self,
        owner_id: u64,
        allocate: impl FnOnce(&mut ActorHeap, &ManagedLayoutRegistry) -> Result<R, String>,
    ) -> Result<R, String> {
        self.heap(owner_id)?;
        let layouts = self.layouts.as_ref();
        self.heaps
            .get_mut(&owner_id)
            .ok_or_else(|| {
                "error[managed_execution.heap]: actor heap insertion was lost".to_string()
            })?
            .with_allocation_transaction(|heap| allocate(heap, layouts))
    }

    /// Lends one materialized actor heap and its immutable admitted layouts.
    pub(crate) fn with_public_materialization<R>(
        &self,
        owner_id: u64,
        materialize: impl FnOnce(&ActorHeap, &ManagedLayoutRegistry) -> Result<R, String>,
    ) -> Result<R, String> {
        materialize(self.heap_ref(owner_id)?, &self.layouts)
    }

    /// Lends a call-scoped context and allocator without eagerly creating a heap.
    pub(crate) fn with_dispatch<R>(
        &mut self,
        owner_id: u64,
        invoke: impl FnOnce(*mut c_void, *const c_void, *const c_void) -> R,
    ) -> R {
        self.last_allocation_error = None;
        let mut context = ManagedAllocationContext {
            collection_requested: 0,
            runtime: self,
            owner_id,
        };
        invoke(
            (&mut context as *mut ManagedAllocationContext).cast(),
            managed_allocate as ManagedAllocator as *const c_void,
            managed_resolve_closure as ManagedClosureResolver as *const c_void,
        )
    }

    /// Validates one managed ABI word against its owner and boundary identity.
    pub(crate) fn validate_boundary_reference(
        &self,
        owner_id: u64,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<(), String> {
        self.boundary_reference(owner_id, boundary_type, value)
            .map(|_| ())
    }

    /// Allocates one public UTF-8 argument in its destination actor heap.
    pub(crate) fn allocate_string_value(
        &mut self,
        owner_id: u64,
        value: &str,
    ) -> Result<i64, String> {
        self.heap(owner_id)?
            .allocate_string(value)
            .map(reference_word)
            .map_err(|error| format!("error[managed_execution.string]: {error}"))
    }

    /// Adopts immutable UTF-8 storage for compiler-proven scalar ingress.
    pub(crate) fn allocate_shared_string_value(
        &mut self,
        owner_id: u64,
        value: bytes::Bytes,
    ) -> Result<i64, String> {
        self.heap(owner_id)?
            .allocate_shared_string(value)
            .map(reference_word)
            .map_err(|error| format!("error[managed_execution.string]: {error}"))
    }

    /// Allocates one public immutable byte argument in its destination actor heap.
    pub(crate) fn allocate_bytes_value(
        &mut self,
        owner_id: u64,
        value: &[u8],
    ) -> Result<i64, String> {
        self.heap(owner_id)?
            .allocate_bytes(value)
            .map(reference_word)
            .map_err(|error| format!("error[managed_execution.bytes]: {error}"))
    }

    /// Allocates one public bitstring argument and its private backing bytes atomically.
    pub(crate) fn allocate_binary_value(
        &mut self,
        owner_id: u64,
        packed: &[u8],
        bit_length: usize,
    ) -> Result<i64, String> {
        self.heap(owner_id)?
            .with_allocation_transaction(|heap| {
                let storage = heap.allocate_bytes(packed)?;
                heap.allocate_binary(storage, 0, bit_length)
            })
            .map(reference_word)
            .map_err(|error| format!("error[managed_execution.binary]: {error}"))
    }

    /// Copies one actor-local managed String into runtime-owned public storage.
    pub(crate) fn materialize_string_value(
        &self,
        owner_id: u64,
        value: i64,
    ) -> Result<String, String> {
        let reference = self.boundary_reference(owner_id, &TvmBoundaryType::String, value)?;
        self.heap_ref(owner_id)?
            .read_string(reference.cast::<ManagedString>())
            .map(str::to_owned)
            .map_err(|error| format!("error[managed_execution.string]: {error}"))
    }

    /// Copies one actor-local managed Bytes value into runtime-owned public storage.
    pub(crate) fn materialize_bytes_value(
        &self,
        owner_id: u64,
        value: i64,
    ) -> Result<Vec<u8>, String> {
        let reference = self.boundary_reference(owner_id, &TvmBoundaryType::Bytes, value)?;
        self.heap_ref(owner_id)?
            .read_bytes(reference.cast::<ManagedBytes>())
            .map(<[u8]>::to_vec)
            .map_err(|error| format!("error[managed_execution.bytes]: {error}"))
    }

    /// Copies one actor-local managed Binary slice into canonical zero-based storage.
    pub(crate) fn materialize_binary_value(
        &self,
        owner_id: u64,
        value: i64,
    ) -> Result<(Vec<u8>, usize), String> {
        let reference = self.boundary_reference(owner_id, &TvmBoundaryType::Binary, value)?;
        let view = self
            .heap_ref(owner_id)?
            .read_binary(reference.cast::<ManagedBinary>())
            .map_err(|error| format!("error[managed_execution.binary]: {error}"))?;
        let byte_length = view.bit_length().checked_add(7).ok_or_else(|| {
            "error[managed_execution.binary]: bit length exceeds host limits".to_string()
        })? / 8;
        let mut packed = vec![0_u8; byte_length];
        for bit in 0..view.bit_length() {
            if view.bit(bit) == Some(true) {
                packed[bit / 8] |= 1 << (7 - bit % 8);
            }
        }
        Ok((packed, view.bit_length()))
    }

    /// Returns occupied bytes and object count for one materialized actor heap.
    #[cfg(test)]
    pub(crate) fn heap_usage(&self, owner_id: u64) -> Option<(usize, usize)> {
        self.heaps
            .get(&owner_id)
            .map(|heap| (heap.allocated_bytes(), heap.object_count()))
    }

    /// Returns the number of actor heaps materialized by managed allocation.
    pub(crate) fn actor_count(&self) -> usize {
        self.heaps.len()
    }

    /// Returns reclaimed heap capacity retained by this shard for focused tests.
    #[cfg(test)]
    pub(crate) fn recycled_heap_count(&self) -> usize {
        self.recycled_heaps.len()
    }

    /// Returns or creates the heap exclusively owned by one protocol actor.
    fn heap(&mut self, owner_id: u64) -> Result<&mut ActorHeap, String> {
        if !self.heaps.contains_key(&owner_id) {
            let owner = ActorId::new(owner_id)
                .map_err(|error| format!("error[managed_execution.owner]: {error}"))?;
            let heap = match self.recycled_heaps.pop() {
                Some(mut heap) => {
                    heap.assign_recycled_owner(owner);
                    heap
                }
                None => ActorHeap::new(owner, self.limits)
                    .map_err(|error| format!("error[managed_execution.heap]: {error}"))?,
            };
            self.heaps.insert(owner_id, heap);
        }
        self.heaps.get_mut(&owner_id).ok_or_else(|| {
            "error[managed_execution.heap]: actor heap insertion was lost".to_string()
        })
    }

    /// Borrows one already materialized actor heap.
    fn heap_ref(&self, owner_id: u64) -> Result<&ActorHeap, String> {
        self.heaps.get(&owner_id).ok_or_else(|| {
            format!("error[managed_execution.reference]: owner {owner_id} has no managed heap")
        })
    }

    /// Decodes one boundary word into a validated owner-local reference.
    fn boundary_reference(
        &self,
        owner_id: u64,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<TvmRef<()>, String> {
        let semantic_id = managed_semantic_id(boundary_type)?.ok_or_else(|| {
            "error[managed_execution.reference_type]: boundary type is not managed".to_string()
        })?;
        let heap = self.heap_ref(owner_id)?;
        let encoded = u64::from_ne_bytes(value.to_ne_bytes());
        heap.validate_abi_reference(encoded, semantic_id)
            .map_err(|error| {
                if error == super::ManagedMemoryError::ManagedTypeMismatch {
                    let actual = heap
                        .abi_reference_semantic_id(encoded)
                        .map(|(_, actual)| format!("{actual:?}"))
                        .unwrap_or_else(|reason| format!("unavailable ({reason})"));
                    format!(
                        "error[managed_execution.reference]: {error}; expected {semantic_id:?} for {boundary_type:?}, actual {actual}"
                    )
                } else {
                    format!("error[managed_execution.reference]: {error}")
                }
            })
    }

    /// Retains managed captures as precise roots and returns transport scalars.
    pub(crate) fn park_continuation_captures(
        &self,
        owner_id: u64,
        continuation_id: u64,
        types: &[TvmBoundaryType],
        values: &[i64],
    ) -> Result<(Vec<i64>, Option<PendingManagedCaptures>), String> {
        if types.len() != values.len() {
            return Err(format!(
                "error[managed_execution.capture_arity]: continuation {continuation_id} declares {} captures but yielded {}",
                types.len(),
                values.len()
            ));
        }
        let owner = ActorId::new(owner_id)
            .map_err(|error| format!("error[managed_execution.owner]: {error}"))?;
        let mut transported = Vec::with_capacity(values.len());
        let mut references = Vec::new();
        let mut positions = Vec::new();
        for (position, (boundary_type, value)) in types.iter().zip(values).enumerate() {
            let Some(semantic_id) = managed_semantic_id(boundary_type)? else {
                transported.push(*value);
                continue;
            };
            let heap = self.heaps.get(&owner_id).ok_or_else(|| {
                format!("error[managed_execution.capture]: owner {owner_id} has no managed heap")
            })?;
            let encoded = u64::from_ne_bytes(value.to_ne_bytes());
            let reference = heap
                .validate_abi_reference(encoded, semantic_id)
                .map_err(|error| {
                    let actual = usize::try_from(encoded)
                        .ok()
                        .and_then(NonZeroUsize::new)
                        .and_then(|encoded| {
                            heap.descriptor(TvmRef::<()>::from_encoded(encoded))
                                .ok()
                                .map(|descriptor| descriptor.semantic_id())
                        });
                    let expected_name = self
                        .layouts
                        .layouts(semantic_id)
                        .first()
                        .map(|layout| layout.canonical_type());
                    let actual_name = actual
                        .and_then(|semantic| self.layouts.layouts(semantic).first())
                        .map(|layout| layout.canonical_type());
                    format!(
                        "error[managed_execution.capture]: continuation {continuation_id} capture {position} expects {boundary_type:?} ({expected_name:?}), received semantic type {actual:?} ({actual_name:?}): {error}"
                    )
                })?;
            positions.push(position);
            references.push(reference);
        }
        if references.is_empty() {
            return Ok((transported, None));
        }
        let continuation = ManagedContinuation::capture(owner, continuation_id, references)
            .map_err(|error| format!("error[managed_execution.capture]: {error}"))?;
        Ok((
            transported,
            Some(PendingManagedCaptures {
                owner,
                continuation,
                positions: positions.into_boxed_slice(),
                capture_count: types.len(),
            }),
        ))
    }

    /// Restores withheld managed roots into their descriptor-declared positions.
    pub(crate) fn restore_continuation_captures(
        &self,
        owner_id: u64,
        continuation_id: u64,
        types: &[TvmBoundaryType],
        transported: &[i64],
        pending: Option<PendingManagedCaptures>,
    ) -> Result<Vec<i64>, String> {
        self.reconstruct_continuation_captures(
            owner_id,
            continuation_id,
            types,
            transported,
            pending.as_ref(),
        )
    }

    /// Reconstructs a debugger snapshot without claiming or consuming roots.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn snapshot_continuation_captures(
        &self,
        owner_id: u64,
        continuation_id: u64,
        types: &[TvmBoundaryType],
        transported: &[i64],
        pending: Option<&PendingManagedCaptures>,
    ) -> Result<Vec<i64>, String> {
        self.reconstruct_continuation_captures(
            owner_id,
            continuation_id,
            types,
            transported,
            pending,
        )
    }

    fn reconstruct_continuation_captures(
        &self,
        owner_id: u64,
        continuation_id: u64,
        types: &[TvmBoundaryType],
        transported: &[i64],
        pending: Option<&PendingManagedCaptures>,
    ) -> Result<Vec<i64>, String> {
        let managed_count = types
            .iter()
            .filter(|boundary_type| boundary_type.is_managed_reference())
            .count();
        if transported.len() != types.len().saturating_sub(managed_count) {
            return Err(format!(
                "error[managed_execution.capture_arity]: continuation {continuation_id} received {} transport captures, expected {}",
                transported.len(),
                types.len().saturating_sub(managed_count)
            ));
        }
        if managed_count == 0 {
            if pending.is_some() {
                return Err(format!(
                    "error[managed_execution.capture_shape]: continuation {continuation_id} retained unexpected managed roots"
                ));
            }
            return Ok(transported.to_vec());
        }
        let pending = pending.ok_or_else(|| {
            format!(
                "error[managed_execution.capture_missing]: continuation {continuation_id} lost its managed roots"
            )
        })?;
        if pending.owner.get() != owner_id
            || pending.continuation.owner() != pending.owner
            || pending.continuation.continuation_id() != continuation_id
            || pending.capture_count != types.len()
            || pending.positions.len() != managed_count
        {
            return Err(format!(
                "error[managed_execution.capture_identity]: continuation {continuation_id} managed roots do not match owner {owner_id}"
            ));
        }
        let heap = self.heaps.get(&owner_id).ok_or_else(|| {
            format!("error[managed_execution.capture]: owner {owner_id} has no managed heap")
        })?;
        let mut scalar_values = transported.iter().copied();
        let mut managed_values = pending.continuation.captures().iter();
        let mut managed_positions = pending.positions.iter().copied();
        let mut restored = Vec::with_capacity(types.len());
        for (position, boundary_type) in types.iter().enumerate() {
            let Some(semantic_id) = managed_semantic_id(boundary_type)? else {
                restored.push(scalar_values.next().expect("transport arity checked above"));
                continue;
            };
            if managed_positions.next() != Some(position) {
                return Err(format!(
                    "error[managed_execution.capture_shape]: continuation {continuation_id} managed position map is invalid"
                ));
            }
            let root = managed_values.next().ok_or_else(|| {
                format!(
                    "error[managed_execution.capture_shape]: continuation {continuation_id} managed root count is invalid"
                )
            })?;
            heap.validate_abi_reference(root.reference().encoded_abi_word(), semantic_id)
                .map_err(|error| format!("error[managed_execution.capture]: {error}"))?;
            restored.push(i64::from_ne_bytes(
                root.reference().encoded_abi_word().to_ne_bytes(),
            ));
        }
        if scalar_values.next().is_some()
            || managed_values.next().is_some()
            || managed_positions.next().is_some()
        {
            return Err(format!(
                "error[managed_execution.capture_shape]: continuation {continuation_id} capture iterators did not finish together"
            ));
        }
        Ok(restored)
    }
}

mod callbacks;

use callbacks::{managed_allocate, managed_resolve_closure};

#[path = "execution/allocation.rs"]
mod allocation;
use allocation::managed_allocate_inner;
