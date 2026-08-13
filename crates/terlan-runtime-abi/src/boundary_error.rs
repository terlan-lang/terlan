use std::error::Error;
use std::fmt;

/// Subsystem that owns a structured Terlan boundary failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorDomain {
    /// Compiler phase orchestration before backend selection.
    CompilerPhase,
    /// NativeIR lowering or native object emission.
    NativeIrEmission,
    /// Native executable-image encoding, inspection, or admission.
    NativeImageAdmission,
    /// NativeBoundary protocol or adapter operation.
    NativeBoundary,
    /// Language-server analysis of an open source document.
    LspAnalysis,
    /// Top-level command parsing or execution.
    CommandExecution,
    /// HTML or template rendering.
    TemplateRendering,
    /// VM-owned actor, session, or protocol operation.
    VmRuntime,
}

/// Typed cross-subsystem failure with stable diagnostic data and an error source.
pub struct BoundaryError {
    domain: ErrorDomain,
    code: String,
    operation: &'static str,
    context: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl PartialEq for BoundaryError {
    fn eq(&self, other: &Self) -> bool {
        self.domain == other.domain
            && self.code == other.code
            && self.operation == other.operation
            && self.context == other.context
            && self.source().map(ToString::to_string) == other.source().map(ToString::to_string)
    }
}

impl Eq for BoundaryError {}

impl BoundaryError {
    /// Creates a structured failure from an already-rendered Terlan diagnostic.
    pub fn message(
        domain: ErrorDomain,
        operation: &'static str,
        rendered: impl Into<String>,
    ) -> Self {
        let context = rendered.into();
        Self {
            domain,
            code: diagnostic_code(&context)
                .unwrap_or("terlan.boundary")
                .to_owned(),
            operation,
            context,
            source: None,
        }
    }

    /// Creates a structured failure while preserving its concrete source.
    pub fn sourced<E>(
        domain: ErrorDomain,
        code: impl Into<String>,
        operation: &'static str,
        context: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            domain,
            code: code.into(),
            operation,
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Returns the subsystem that owns the failure.
    pub const fn domain(&self) -> ErrorDomain {
        self.domain
    }

    /// Returns the stable machine-readable diagnostic code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the failed boundary operation.
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns structured context without flattening the source chain.
    pub fn context(&self) -> &str {
        &self.context
    }
}

impl fmt::Debug for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundaryError")
            .field("domain", &self.domain)
            .field("code", &self.code)
            .field("operation", &self.operation)
            .field("context", &self.context)
            .field("has_source", &self.source.is_some())
            .finish()
    }
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.context)
    }
}

impl Error for BoundaryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl From<BoundaryError> for String {
    fn from(error: BoundaryError) -> Self {
        error.to_string()
    }
}

fn diagnostic_code(rendered: &str) -> Option<&str> {
    rendered
        .strip_prefix("error[")
        .and_then(|remainder| remainder.split_once("]:"))
        .map(|(code, _)| code)
}

#[cfg(test)]
#[path = "boundary_error_test.rs"]
mod boundary_error_test;
