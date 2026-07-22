#![allow(dead_code)]

use std::sync::{Arc, Mutex};

/// Cloneable VM service handle for session state shared by request shards.
///
/// The handle keeps synchronization inside the session service rather than in
/// generated-code execution state. Shards carry only this explicit service
/// capability and never expose or retain its lock guard.
#[derive(Clone, Debug)]
pub(crate) struct VmHttpSessionService {
    runtime: Arc<Mutex<VmHttpSessionRuntime>>,
}

impl VmHttpSessionService {
    pub(crate) fn new(runtime: VmHttpSessionRuntime) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
        }
    }

    pub(crate) fn with_runtime<T>(
        &self,
        operation: impl FnOnce(&mut VmHttpSessionRuntime) -> T,
    ) -> Result<T, String> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "HTTP session service lock poisoned".to_string())?;
        Ok(operation(&mut runtime))
    }
}

#[path = "http_session/diagnostics.rs"]
mod diagnostics;

#[path = "http_session/live_template_payload.rs"]
mod live_template_payload;

#[path = "http_session/live_template_command.rs"]
pub(crate) mod live_template_command;

#[path = "http_session/live_template_repeated_diff.rs"]
pub(crate) mod live_template_repeated_diff;

#[path = "http_session/live_template_response.rs"]
pub(crate) mod live_template_response;

#[cfg(test)]
#[path = "http_session_test.rs"]
mod http_session_test;

#[cfg(test)]
#[path = "http_session_live_template_payload_test.rs"]
mod http_session_live_template_payload_test;

#[cfg(test)]
#[path = "http_session_live_template_response_test.rs"]
mod http_session_live_template_response_test;
include!("http_session_part_001.rs");
include!("http_session_part_002.rs");
include!("http_session_part_003.rs");
