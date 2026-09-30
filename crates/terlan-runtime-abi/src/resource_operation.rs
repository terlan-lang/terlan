//! Source contracts for package-owned resource adapters.

/// Describes an operation whose resource transport is supplied by its host.
///
/// Registration is not authorization: callers must still enforce resource
/// ownership, worker admission, cancellation, and argument types. In particular,
/// this descriptor does not make resources valid in the value-only ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeResourceOperation {
    /// Exact operation identifier declared by the package source.
    pub operation: &'static str,
    /// Runtime argument count, including a receiver when present.
    pub arity: usize,
    /// Fully qualified source type owned by this resource adapter.
    pub resource_type: &'static str,
    /// Whether success returns a resource, rather than recursively owned data.
    pub returns_resource: bool,
    /// Source error record for a Result-returning operation; None returns directly.
    pub result_error: Option<&'static str>,
    /// Whether execution requires exclusive access to the first argument.
    pub mutates_receiver: bool,
}

/// Host-local input or output of a typed resource adapter.
///
/// A borrowed resource may be used as R for inputs and an owned resource for
/// outputs. This type is deliberately not serializable: hosts must turn a
/// resource into an owner-checked handle before crossing a worker boundary.
#[derive(Debug, PartialEq)]
pub enum NativeResourceValue<R> {
    /// Recursively owned data, without resource handles or host pointers.
    Value(crate::NativeValue),
    /// A resource whose ownership remains with the calling host.
    Resource(R),
}
