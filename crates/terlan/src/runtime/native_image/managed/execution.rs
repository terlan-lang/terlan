//! Owner-scoped managed execution context for generated native calls.

use std::collections::HashMap;
use std::ffi::c_void;
use std::num::NonZeroUsize;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use crate::runtime::native_image::{
    TvmBoundaryType, TvmCallableDescriptor, TvmManagedCollectionDescriptor,
    TvmManagedLayoutDescriptor,
};
use crate::runtime::vm::http_session::VmHttpSessionService;

use super::{
    ActorHeap, ActorId, AtomIndex, HeapLimits, ManagedBinary, ManagedBytes, ManagedClosure,
    ManagedClosureDispatchTable, ManagedClosureImageGeneration, ManagedContinuation,
    ManagedLayoutRegistry, ManagedMailboxFragment, ManagedRoot, ManagedString, TvmRef,
    MANAGED_ALLOCATION_FAILED_STATUS, MAX_MANAGED_AGGREGATE_ABI_BYTES,
};

#[path = "execution/actor_transfer.rs"]
mod actor_transfer;
pub(crate) use actor_transfer::ManagedActorTransfer;
#[path = "execution/abi_types.rs"]
mod abi_types;
use abi_types::{managed_semantic_id, reference_word, ManagedAllocator, ManagedClosureResolver};
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
}

/// Stack-bound context passed only for the duration of one generated dispatch.
struct ManagedAllocationContext {
    /// Exclusively borrowed runtime that owns the destination actor heap.
    runtime: *mut ManagedExecutionRuntime,
    /// Nonzero actor identity selected for this synchronous dispatch.
    owner_id: u64,
}

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
        self.heap_ref(owner_id)?
            .validate_abi_reference(u64::from_ne_bytes(value.to_ne_bytes()), semantic_id)
            .map_err(|error| format!("error[managed_execution.reference]: {error}"))
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
                    format!(
                        "error[managed_execution.capture]: continuation {continuation_id} capture {position} expects {boundary_type:?}, received semantic type {actual:?}: {error}"
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

include!("execution/callbacks.rs");

/// Performs pointer checks and converts bounded ABI storage into Rust slices.
#[allow(unsafe_code)]
fn managed_allocate_inner(
    context: *mut c_void,
    layout: *const u8,
    layout_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let Ok(layout_len) = usize::try_from(layout_len) else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    let Ok(field_count) = usize::try_from(field_count) else {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    };
    if context.is_null()
        || result.is_null()
        || layout.is_null()
        || !(context as *const ManagedAllocationContext).is_aligned()
        || !result.is_aligned()
        || layout_len == 0
        || layout_len > MAX_MANAGED_AGGREGATE_ABI_BYTES
        || field_count > MAX_AGGREGATE_FIELD_WORDS
        || (field_count != 0 && (fields.is_null() || !fields.is_aligned()))
    {
        return MANAGED_ALLOCATION_FAILED_STATUS;
    }
    // SAFETY: The generated caller keeps all bounded buffers and the stack
    // context alive for this synchronous callback. Null and length checks above
    // establish the slice preconditions.
    let (context, layout, fields) = unsafe {
        let context = &mut *context.cast::<ManagedAllocationContext>();
        let layout = std::slice::from_raw_parts(layout, layout_len);
        let fields = if field_count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(fields, field_count)
        };
        (context, layout, fields)
    };
    // SAFETY: `with_dispatch` created this runtime pointer from an exclusive
    // borrow and does not expose it beyond the synchronous invocation.
    let runtime = unsafe { &mut *context.runtime };
    let allocation = (|| {
        runtime.heap(context.owner_id)?;
        let layouts = runtime.layouts.as_ref();
        let closure_dispatch = runtime.closure_dispatch.as_deref();
        let http_sessions = runtime.http_sessions.as_ref();
        let heap = runtime.heaps.get_mut(&context.owner_id).ok_or_else(|| {
            "error[managed_execution.heap]: actor heap insertion was lost".to_string()
        })?;
        heap.with_allocation_transaction(|heap| {
            if super::is_closure_allocation(layout) {
                let dispatch = closure_dispatch
                    .ok_or("managed closure allocation has no admitted image dispatch")?;
                super::execute_closure_allocation(heap, dispatch, layout, fields)
                    .map_err(|error| error.to_string())
            } else if super::is_managed_operation(layout) {
                super::execute_managed_operation_with_context(
                    heap,
                    layouts,
                    http_sessions,
                    layout,
                    fields,
                )
                .map_err(|error| {
                    let family = std::str::from_utf8(layout.get(..4).unwrap_or_default())
                        .unwrap_or("invalid");
                    let operation = layout.get(6).copied().unwrap_or_default();
                    let semantic_context = match family {
                        "TVME" => {
                            let expected = layout.get(8..24).unwrap_or_default();
                            let actual = fields
                                .iter()
                                .enumerate()
                                .map(|(index, word)| {
                                    let reference = usize::try_from(u64::from_ne_bytes(
                                        word.to_ne_bytes(),
                                    ))
                                        .ok()
                                        .and_then(NonZeroUsize::new)
                                        .map(TvmRef::<()>::from_encoded);
                                    let resolved = reference
                                        .ok_or(super::ManagedMemoryError::UnknownReference)
                                        .and_then(|reference| heap.descriptor(reference))
                                        .map(|descriptor| descriptor.semantic_id().bytes())
                                        .map_err(|error| error.to_string());
                                    (index, resolved)
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "; expected semantic {expected:?}, operand words {fields:?}, actual semantics {actual:?}"
                            )
                        }
                        "TVMC" => {
                            let expected = layout.get(8..24).unwrap_or_default();
                            let actual = fields
                                .iter()
                                .filter_map(|word| {
                                    usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
                                        .ok()
                                        .and_then(NonZeroUsize::new)
                                })
                                .filter_map(|word| {
                                    heap.descriptor(TvmRef::<()>::from_encoded(word))
                                        .ok()
                                        .map(|descriptor| descriptor.semantic_id().bytes())
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "; expected collection semantic {expected:?}, actual reference semantics {actual:?}, admitted collections {:?}",
                                layouts.collection_inventory()
                            )
                        }
                        "TVMP" | "TVMO" => {
                            let expected_bytes = layout
                                .get(8..24)
                                .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
                                .unwrap_or_default();
                            let expected = super::SemanticTypeId::from_bytes(expected_bytes);
                            let actual = fields.first().and_then(|word| {
                                usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
                                    .ok()
                                    .and_then(NonZeroUsize::new)
                                    .and_then(|word| {
                                        heap.descriptor(TvmRef::<()>::from_encoded(word)).ok()
                                    })
                                    .map(|descriptor| {
                                        (
                                            descriptor.semantic_id().bytes(),
                                            descriptor.fingerprint(),
                                        )
                                    })
                            });
                            let admitted = layouts
                                .layouts(expected)
                                .iter()
                                .map(|descriptor| {
                                    (
                                        descriptor.canonical_type(),
                                        descriptor.variant_name(),
                                        descriptor.managed().fingerprint(),
                                    )
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "; expected aggregate semantic {expected_bytes:?}, actual {actual:?}, admitted layouts {admitted:?}"
                            )
                        }
                        _ => String::new(),
                    };
                    format!("{error}; managed operation {family}/{operation}{semantic_context}")
                })
            } else {
                heap.allocate_managed_words_abi(layout, fields)
                    .map_err(|error| error.to_string())
            }
        })
    })()
    .map_err(|error| format!("error[managed_execution.allocate]: {error}"));
    match allocation {
        Ok(reference) => {
            // SAFETY: Non-null caller-owned result storage remains live for the
            // callback and is written only after complete heap publication.
            unsafe { result.write(reference) };
            0
        }
        Err(error) => {
            runtime.last_allocation_error = Some(error);
            MANAGED_ALLOCATION_FAILED_STATUS
        }
    }
}
