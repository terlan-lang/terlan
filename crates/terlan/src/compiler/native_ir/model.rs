//! Backend-independent value, control, and callable nodes for NativeIR.

use std::sync::Arc;

use crate::runtime::native_image::managed::{ManagedAggregateDescriptor, SemanticTypeId};

/// Closed value representations carried by `terlan-native-v2`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeType {
    Unit,
    Int,
    Float,
    Bool,
    Atom,
    StringRef,
    BytesRef,
    BinaryRef,
    ManagedRef(SemanticTypeId),
}

impl NativeType {
    /// Reports whether this type is an actor-local relocating managed reference.
    pub(crate) fn is_managed_reference(self) -> bool {
        matches!(
            self,
            Self::StringRef | Self::BytesRef | Self::BinaryRef | Self::ManagedRef(_)
        )
    }

    /// Projects the compiler representation into its canonical runtime type.
    pub(crate) fn boundary_type(self) -> crate::runtime::native_image::TvmBoundaryType {
        use crate::runtime::native_image::TvmBoundaryType;

        match self {
            Self::Unit => TvmBoundaryType::Unit,
            Self::Int => TvmBoundaryType::Int,
            Self::Float => TvmBoundaryType::Float,
            Self::Bool => TvmBoundaryType::Bool,
            Self::Atom => TvmBoundaryType::Atom,
            Self::StringRef => TvmBoundaryType::String,
            Self::BytesRef => TvmBoundaryType::Bytes,
            Self::BinaryRef => TvmBoundaryType::Binary,
            Self::ManagedRef(identity) => TvmBoundaryType::Managed(identity.bytes()),
        }
    }
}

/// Typed native expression independent from any code-generation backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeExpr {
    Unit,
    Int(i64),
    /// One finite IEEE-754 value carried as raw bits across the i64 ABI slot.
    Float(u64),
    Bool(bool),
    /// One immutable UTF-8 literal allocated in the current actor heap.
    StringLiteral {
        encoded: Arc<[u8]>,
    },
    /// One checked actor-heap operation executed through the bounded managed callback.
    ManagedOperation {
        encoded: Arc<[u8]>,
        args: Vec<NativeExpr>,
    },
    /// One image-local callable paired with captures owned by the actor heap.
    MakeClosure {
        encoded: Arc<[u8]>,
        captures: Vec<NativeExpr>,
    },
    Param(usize),
    /// One fixed algebraic constructor with its canonical runtime layout.
    Construct {
        descriptor: Arc<ManagedAggregateDescriptor>,
        encoded_layout: Arc<[u8]>,
        fields: Vec<NativeExpr>,
    },
    Call {
        function: usize,
        args: Vec<NativeExpr>,
    },
    /// One validated owned-closure invocation routed through image-local dispatch.
    #[allow(dead_code)]
    InvokeClosure {
        callee: Box<NativeExpr>,
        args: Vec<NativeExpr>,
        parameter_types: Vec<NativeType>,
        result_type: NativeType,
    },
    /// A call whose result is returned directly and whose transition may be forwarded.
    TailCall {
        function: usize,
        args: Vec<NativeExpr>,
    },
    /// A terminal suspending call followed by a caller-owned continuation.
    CallThen {
        function: usize,
        args: Vec<NativeExpr>,
        callee_continuation_id: u64,
        callee_capture_count: usize,
        continuation_id: u64,
        values: Vec<NativeExpr>,
        resume: Box<NativeExpr>,
    },
    Neg(Box<NativeExpr>),
    FloatNeg(Box<NativeExpr>),
    IntToFloat(Box<NativeExpr>),
    Not(Box<NativeExpr>),
    Binary {
        operator: NativeBinaryOperator,
        operand_type: NativeType,
        left: Box<NativeExpr>,
        right: Box<NativeExpr>,
    },
    Let {
        bindings: Vec<NativeExpr>,
        body: Box<NativeExpr>,
    },
    If {
        clauses: Vec<(NativeExpr, NativeExpr)>,
    },
    /// One protected native expression with status-aware success/failure selection.
    /// Cleanup expressions execute exactly once on every selected or propagating path.
    Try {
        protected: Box<NativeExpr>,
        success: Box<NativeExpr>,
        failure: Box<NativeExpr>,
        cleanup: Vec<NativeExpr>,
    },
    Suspend {
        operation: NativeTransitionOperation,
        arguments: Vec<NativeExpr>,
        continuation_id: u64,
        values: Vec<NativeExpr>,
    },
}

/// VM-owned operation emitted when native code suspends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTransitionOperation {
    Yield,
    Send,
    SendTyped,
    Receive,
    ReceiveTyped,
    Spawn,
    Timer,
    Link,
    Monitor,
    Resource,
    Cancellation,
    Failure,
    Scheduling,
    /// One declared asynchronous capability request serviced outside the shard.
    Capability,
}

/// Arithmetic and comparison operations in the native scalar profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeBinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}
