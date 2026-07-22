//! Canonical managed-memory identities, opaque references, and failures.

use std::fmt;
use std::marker::PhantomData;
use std::num::{NonZeroU64, NonZeroUsize};

/// Stable identity of the actor that exclusively owns one managed heap.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActorId(NonZeroU64);

impl ActorId {
    /// Creates a nonzero actor identity.
    pub fn new(value: u64) -> Result<Self, ManagedMemoryError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(ManagedMemoryError::InvalidActorId)
    }

    /// Returns the stable numeric actor identity.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// Opaque pointer-width reference to one actor-local managed object.
///
/// The runtime may rewrite this value at a safepoint. Application code cannot
/// construct it, inspect its address, or retain it outside the owning actor.
pub struct TvmRef<T> {
    encoded: NonZeroUsize,
    marker: PhantomData<fn() -> T>,
}

impl<T> TvmRef<T> {
    /// Erases the compile-time pointee marker while preserving reference identity.
    pub fn erase(self) -> TvmRef<()> {
        TvmRef::from_encoded(self.encoded)
    }

    /// Reattaches a runtime-validated compile-time marker to an erased reference.
    pub(crate) fn cast<U>(self) -> TvmRef<U> {
        TvmRef::from_encoded(self.encoded)
    }

    /// Creates a typed reference from its runtime-private encoded identity.
    pub(crate) fn from_encoded(encoded: NonZeroUsize) -> Self {
        Self {
            encoded,
            marker: PhantomData,
        }
    }

    /// Returns the runtime-private encoded identity.
    pub(super) fn encoded(self) -> NonZeroUsize {
        self.encoded
    }

    /// Returns one internal ABI word after runtime ownership validation.
    pub(crate) fn encoded_abi_word(self) -> u64 {
        self.encoded.get() as u64
    }
}

impl<T> Clone for TvmRef<T> {
    /// Copies the opaque managed reference.
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TvmRef<T> {}

impl<T> fmt::Debug for TvmRef<T> {
    /// Formats the reference without exposing its runtime-private address.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TvmRef(<opaque>)")
    }
}

impl<T> PartialEq for TvmRef<T> {
    /// Compares managed-reference identity within one heap generation.
    fn eq(&self, other: &Self) -> bool {
        self.encoded == other.encoded
    }
}

impl<T> Eq for TvmRef<T> {}

/// Typed failure produced by managed-layout, root, allocation, and collection checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManagedMemoryError {
    InvalidActorId,
    UnsupportedPointerWidth,
    EmptySemanticIdentity,
    InvalidLayoutSize,
    InvalidLayoutAlignment,
    InvalidReferenceMap,
    LayoutMismatch,
    AllocationLimitExceeded,
    CollectionBudgetExceeded,
    CrossActorReference,
    StaleReference,
    UnknownReference,
    MissingStackMap,
    InvalidStackMap,
    BorrowedValueAtSafepoint,
    InvalidContinuation,
    InvalidMailboxTransfer,
    MessageTransferBudgetExceeded,
    CorruptedRelocationMetadata,
    InvalidUtf8,
    InvalidSequenceLength,
    InvalidBitRange,
    ManagedTypeMismatch,
    EmptyAtomIdentity,
    InvalidAtomIdentity,
    TooManyAtoms,
    UnknownAtom,
    InvalidAggregateShape,
    InvalidAggregateArity,
    InvalidAggregateField,
    /// Encoded native aggregate allocation descriptor is malformed.
    InvalidAggregateAbi,
    /// A bounded managed native operation rejected its typed payload.
    InvalidManagedOperation,
    InvalidClosure,
    UnknownClosureCallable,
    StaleClosureGeneration,
    ClosureSignatureMismatch,
    ClosureCaptureMismatch,
    InvalidVariantDiscriminant,
    InvalidManagedScalar,
    CollectionTooLarge,
    CollectionIndexOutOfBounds,
    CorruptedCollection,
}

impl fmt::Display for ManagedMemoryError {
    /// Renders one stable managed-memory diagnostic.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidActorId => "error[tvm.managed.actor]: actor identity must be nonzero",
            Self::UnsupportedPointerWidth => {
                "error[tvm.managed.pointer_width]: managed ABI 1 requires a 64-bit target"
            }
            Self::EmptySemanticIdentity => {
                "error[tvm.managed.semantic_identity]: canonical type identity is empty"
            }
            Self::InvalidLayoutSize => "error[tvm.managed.layout_size]: invalid object size",
            Self::InvalidLayoutAlignment => {
                "error[tvm.managed.layout_alignment]: invalid object alignment"
            }
            Self::InvalidReferenceMap => {
                "error[tvm.managed.reference_map]: invalid managed-reference map"
            }
            Self::LayoutMismatch => "error[tvm.managed.layout]: object layout does not match",
            Self::AllocationLimitExceeded => {
                "error[tvm.managed.allocation_limit]: actor heap limit exceeded"
            }
            Self::CollectionBudgetExceeded => {
                "error[tvm.managed.collection_budget]: collection work budget exceeded"
            }
            Self::CrossActorReference => {
                "error[tvm.managed.owner]: managed reference belongs to another actor heap"
            }
            Self::StaleReference => {
                "error[tvm.managed.stale_reference]: managed reference predates relocation"
            }
            Self::UnknownReference => {
                "error[tvm.managed.unknown_reference]: managed reference is not allocated"
            }
            Self::MissingStackMap => {
                "error[tvm.managed.stack_map_missing]: safepoint has no precise stack map"
            }
            Self::InvalidStackMap => "error[tvm.managed.stack_map]: malformed precise stack map",
            Self::BorrowedValueAtSafepoint => {
                "error[tvm.managed.borrowed_safepoint]: borrowed value crosses a safepoint"
            }
            Self::InvalidContinuation => {
                "error[tvm.managed.continuation]: continuation capture does not match its map"
            }
            Self::InvalidMailboxTransfer => {
                "error[tvm.managed.mailbox_transfer]: managed mailbox transfer is invalid"
            }
            Self::MessageTransferBudgetExceeded => {
                "error[tvm.managed.mailbox_budget]: managed message exceeds its copy budget"
            }
            Self::CorruptedRelocationMetadata => {
                "error[tvm.managed.relocation]: corrupted relocation metadata"
            }
            Self::InvalidUtf8 => "error[tvm.managed.utf8]: string payload is not valid UTF-8",
            Self::InvalidSequenceLength => {
                "error[tvm.managed.sequence_length]: sequence length is not representable"
            }
            Self::InvalidBitRange => {
                "error[tvm.managed.bit_range]: binary slice exceeds its backing bytes"
            }
            Self::ManagedTypeMismatch => {
                "error[tvm.managed.type]: managed reference has the wrong semantic type"
            }
            Self::EmptyAtomIdentity => "error[tvm.atom.empty]: atom identity must not be empty",
            Self::InvalidAtomIdentity => {
                "error[tvm.atom.identity]: atom identity contains forbidden characters"
            }
            Self::TooManyAtoms => "error[tvm.atom.capacity]: image atom table exceeds u32 capacity",
            Self::UnknownAtom => "error[tvm.atom.unknown]: atom is not present in this image",
            Self::InvalidAggregateShape => {
                "error[tvm.managed.aggregate_shape]: aggregate shape is invalid"
            }
            Self::InvalidAggregateArity => {
                "error[tvm.managed.aggregate_arity]: aggregate field count does not match"
            }
            Self::InvalidAggregateField => {
                "error[tvm.managed.aggregate_field]: aggregate field does not match its layout"
            }
            Self::InvalidAggregateAbi => {
                "error[tvm.managed.aggregate_abi]: aggregate allocation descriptor is malformed"
            }
            Self::InvalidManagedOperation => {
                "error[tvm.managed.operation]: managed native operation is invalid"
            }
            Self::InvalidClosure => {
                "error[tvm.managed.closure]: managed closure representation is invalid"
            }
            Self::UnknownClosureCallable => {
                "error[tvm.managed.closure_callable]: closure callable is not admitted by this image"
            }
            Self::StaleClosureGeneration => {
                "error[tvm.managed.closure_generation]: closure belongs to another image generation"
            }
            Self::ClosureSignatureMismatch => {
                "error[tvm.managed.closure_signature]: closure invocation signature does not match"
            }
            Self::ClosureCaptureMismatch => {
                "error[tvm.managed.closure_capture]: closure capture environment does not match"
            }
            Self::InvalidVariantDiscriminant => {
                "error[tvm.managed.variant]: constructor discriminant is outside its union"
            }
            Self::InvalidManagedScalar => {
                "error[tvm.managed.scalar]: scalar payload is not a valid ABI value"
            }
            Self::CollectionTooLarge => {
                "error[tvm.managed.collection_size]: collection exceeds its bounded shape"
            }
            Self::CollectionIndexOutOfBounds => {
                "error[tvm.managed.collection_index]: collection index is out of bounds"
            }
            Self::CorruptedCollection => {
                "error[tvm.managed.collection]: collection representation is malformed"
            }
        })
    }
}

impl std::error::Error for ManagedMemoryError {}
