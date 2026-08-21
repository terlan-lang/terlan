use std::fmt;

use serde::{Deserialize, Serialize};

/// Compiler-preserved Terlan source identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub module: String,
    pub function: String,
    pub file: String,
    pub line: u32,
}

/// Validated W3C trace context used at the HTTP boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    pub version: u8,
    pub trace_id: String,
    pub parent_span_id: String,
    pub sampled: bool,
}

impl TraceContext {
    /// Parses and validates one lowercase W3C `traceparent` header value.
    pub fn parse_traceparent(value: &str) -> Result<Self, TraceparentError> {
        let mut parts = value.split('-');
        let (Some(version), Some(trace_id), Some(parent_span_id), Some(flags), None) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(TraceparentError::Shape);
        };
        if version.len() != 2
            || trace_id.len() != 32
            || parent_span_id.len() != 16
            || flags.len() != 2
            || ![version, trace_id, parent_span_id, flags]
                .iter()
                .all(|part| {
                    part.bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                })
        {
            return Err(TraceparentError::Encoding);
        }
        let version = u8::from_str_radix(version, 16).map_err(|_| TraceparentError::Encoding)?;
        if version == u8::MAX {
            return Err(TraceparentError::Version);
        }
        if trace_id.bytes().all(|byte| byte == b'0')
            || parent_span_id.bytes().all(|byte| byte == b'0')
        {
            return Err(TraceparentError::ZeroIdentifier);
        }
        let flags = u8::from_str_radix(flags, 16).map_err(|_| TraceparentError::Encoding)?;
        Ok(Self {
            version,
            trace_id: trace_id.into(),
            parent_span_id: parent_span_id.into(),
            sampled: flags & 1 == 1,
        })
    }
}

/// Stable reasons a `traceparent` value can be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceparentError {
    Shape,
    Encoding,
    Version,
    ZeroIdentifier,
}

impl fmt::Display for TraceparentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid traceparent: {self:?}")
    }
}

impl std::error::Error for TraceparentError {}

/// Read-only identity supplied by the host for one admitted request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestContext {
    pub service: String,
    pub request_id: String,
    pub connection_id: String,
    pub route_id: String,
    pub handler_id: String,
    pub release_id: String,
    pub actor_id: Option<String>,
    pub source: SourceIdentity,
    pub trace: Option<TraceContext>,
}

/// Lifecycle state of the parent work that is creating a child actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkOutcome {
    Active,
    Cancelled,
    TimedOut,
}

/// Selects whether child work inherits request-local identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextDisposition {
    Nested,
    Detached,
}

impl RequestContext {
    /// Propagates identity according to the portable actor contract.
    ///
    /// Nested work inherits request and trace identity. Detached work retains
    /// only deployment/source identity and starts outside the request trace.
    /// Cancelled or timed-out parents cannot admit more nested work.
    pub fn child(
        &self,
        actor_id: impl Into<String>,
        disposition: ContextDisposition,
        parent: WorkOutcome,
    ) -> Option<Self> {
        if disposition == ContextDisposition::Nested && parent != WorkOutcome::Active {
            return None;
        }
        let mut child = self.clone();
        child.actor_id = Some(actor_id.into());
        if disposition == ContextDisposition::Detached {
            child.request_id.clear();
            child.connection_id.clear();
            child.route_id.clear();
            child.handler_id.clear();
            child.trace = None;
        }
        Some(child)
    }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;
