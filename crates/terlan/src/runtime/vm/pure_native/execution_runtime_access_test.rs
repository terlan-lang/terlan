use super::{ManagedExecutionRuntime, PureNativeExecutionRuntime};

impl PureNativeExecutionRuntime {
    /// Creates empty execution state with default managed-memory limits.
    pub(crate) fn runtime_default() -> Result<Self, String> {
        ManagedExecutionRuntime::runtime_default().map(Self::from_managed)
    }
}
