//! Generic owner-checked storage shared by native packages and VM services.

/// Opaque resource handle handed to Terlan-side code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeResourceHandle {
    /// Stable slot identifier inside the adapter-owned registry.
    pub id: u64,
    /// Generation tag used to reject stale handles after slot reuse.
    pub generation: u64,
}

/// Reserved owner id for trusted calls without actor context.
pub const SYSTEM_RESOURCE_OWNER: u64 = 0;

/// Stable resource-registry error returned by handle operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceError {
    code: &'static str,
    message: String,
}

impl ResourceError {
    /// Builds a resource-registry error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    ///
    /// Output:
    /// - A `ResourceError` suitable for native bridge diagnostics.
    ///
    /// Transformation:
    /// - Stores stable error fields without exposing backend resource details.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: resource error.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: resource error.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Builds a stale-handle resource error.
///
/// Inputs:
/// - `handle`: rejected opaque handle.
///
/// Output:
/// - `ResourceError` with stable code `resource.stale_handle`.
///
/// Transformation:
/// - Converts a failed liveness lookup into stable diagnostic fields.
fn stale_error(handle: NativeResourceHandle) -> ResourceError {
    ResourceError::new(
        "resource.stale_handle",
        format!(
            "NativeBoundary resource handle {} generation {} is not live.",
            handle.id, handle.generation
        ),
    )
}

fn owner_error(
    handle: NativeResourceHandle,
    owner_process_id: u64,
    caller_process_id: u64,
) -> ResourceError {
    ResourceError::new(
        "resource.owner",
        format!(
            "NativeBoundary resource handle {} belongs to process {}, not process {}.",
            handle.id, owner_process_id, caller_process_id
        ),
    )
}

mod registry;
pub use registry::ResourceRegistry;
