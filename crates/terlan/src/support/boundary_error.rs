//! Compatibility path and subsystem wrappers for canonical runtime ABI errors.

pub use terlan_runtime_abi::{BoundaryError, ErrorDomain};

/// Typed failure produced by a repository quality executable.
pub struct QualityError(BoundaryError);

impl QualityError {
    /// Creates a quality failure from one stable rendered diagnostic.
    pub fn message(operation: &'static str, rendered: impl Into<String>) -> Self {
        Self(BoundaryError::message(
            ErrorDomain::CommandExecution,
            operation,
            rendered,
        ))
    }

    /// Creates a quality failure while preserving its concrete source.
    pub fn sourced<E>(
        code: impl Into<String>,
        operation: &'static str,
        context: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self(BoundaryError::sourced(
            ErrorDomain::CommandExecution,
            code,
            operation,
            context,
            source,
        ))
    }

    /// Returns the stable machine-readable diagnostic code.
    pub fn code(&self) -> &str {
        self.0.code()
    }
}

impl std::fmt::Debug for QualityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::fmt::Display for QualityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for QualityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.0)
    }
}

impl std::ops::Deref for QualityError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.context()
    }
}

impl From<String> for QualityError {
    fn from(rendered: String) -> Self {
        Self::message("run quality validation", rendered)
    }
}

impl From<&str> for QualityError {
    fn from(rendered: &str) -> Self {
        rendered.to_owned().into()
    }
}

impl From<QualityError> for String {
    fn from(error: QualityError) -> Self {
        error.to_string()
    }
}

/// Result returned by typed repository quality executables.
pub type QualityResult<T> = Result<T, QualityError>;
