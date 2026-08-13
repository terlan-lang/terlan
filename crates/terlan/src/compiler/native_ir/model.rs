//! Backend-independent value, control, and callable nodes for NativeIR.

use std::sync::Arc;

use crate::runtime::native_image::managed::{ManagedAggregateDescriptor, SemanticTypeId};

/// Closed value representations carried by `terlan-native-v2`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

/// One possible callee resume identity wrapped by a suspension-aware call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeCallResume {
    pub(crate) callee_continuation_id: u64,
    pub(crate) callee_capture_count: usize,
    /// VM-owned completion entry pushed while the callee remains parked.
    pub(crate) continuation_id: u64,
    /// First caller-owned value appended after the callee's captures.
    /// Recursive component edges can skip an already-composed prefix while a
    /// surrounding call appends only its newly introduced frame suffix.
    pub(crate) caller_value_start: usize,
}

/// One target-qualified resume edge of a dynamic closure call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeDynamicCallResume {
    pub(crate) callee_export_id: u64,
    pub(crate) callee_continuation_id: u64,
    pub(crate) callee_capture_count: usize,
    pub(crate) continuation_id: u64,
}

/// Typed native expression independent from any code-generation backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeExpr {
    Unit,
    Int(i64),
    /// One finite IEEE-754 value carried as raw bits across the i64 ABI slot.
    Float(u64),
    Bool(bool),
    /// One semantic atom resolved to the image-generation-local compact index
    /// while the native object is emitted.
    AtomLiteral(Arc<str>),
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
    InvokeClosure {
        callee: Box<NativeExpr>,
        args: Vec<NativeExpr>,
        parameter_types: Vec<NativeType>,
        result_type: NativeType,
    },
    /// A dynamic closure call whose caller completion is retained by the VM.
    InvokeClosureThen {
        callee: Box<NativeExpr>,
        args: Vec<NativeExpr>,
        parameter_types: Vec<NativeType>,
        result_type: NativeType,
        resumes: Vec<NativeDynamicCallResume>,
        completion_continuation_id: u64,
        completion_function: Option<usize>,
        values: Vec<NativeExpr>,
    },
    /// A call whose result is returned directly and whose transition may be forwarded.
    TailCall {
        function: usize,
        args: Vec<NativeExpr>,
        /// Stable resume entry used when the VM reduction budget is exhausted.
        ///
        /// `None` forwards an already-suspending terminal call without adding
        /// a recursive-component scheduler boundary.
        yield_continuation_id: Option<u64>,
    },
    /// A terminal suspending call followed by a caller-owned continuation.
    CallThen {
        function: usize,
        args: Vec<NativeExpr>,
        resumes: Vec<NativeCallResume>,
        /// Caller continuation entered when the callee completes without
        /// parking. Application lowering resolves its function index.
        completion_continuation_id: u64,
        completion_function: Option<usize>,
        values: Vec<NativeExpr>,
    },
    /// A shared continuation body whose completion or transition is returned
    /// directly by the current continuation.
    ///
    /// Application lowering resolves this node to `TailCall` after shared
    /// continuation identities have been materialized.
    ContinuationTailCall {
        continuation_id: u64,
        args: Vec<NativeExpr>,
    },
    Neg(Box<NativeExpr>),
    FloatNeg(Box<NativeExpr>),
    FloatFloor(Box<NativeExpr>),
    FloatCeil(Box<NativeExpr>),
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
    Debug,
    Identity,
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
