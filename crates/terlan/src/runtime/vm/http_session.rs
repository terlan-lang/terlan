use std::sync::{Arc, Mutex};

use std::collections::BTreeMap;

use super::actor::VmActorRuntime;
#[cfg(test)]
use super::live_template_protocol::VmLiveTemplateProtocolManifest;
use super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use super::table::{VmTableAccess, VmTableEvent, VmTableId, VmTableStore};
use super::ReplValue;
use diagnostics::crashed_session_actor_diagnostic;
#[cfg(test)]
use live_template_payload::validate_live_template_patch_payload;
#[cfg(test)]
pub(crate) use live_template_payload::VmHttpSessionLiveTemplateSourceSpan;

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
#[cfg(test)]
mod live_template_payload;

#[path = "http_session/live_template_command.rs"]
#[cfg(test)]
pub(crate) mod live_template_command;

#[path = "http_session/live_template_repeated_diff.rs"]
#[cfg(test)]
pub(crate) mod live_template_repeated_diff;

#[path = "http_session/live_template_response.rs"]
#[cfg(test)]
pub(crate) mod live_template_response;

#[cfg(test)]
#[path = "http_session_test.rs"]
#[cfg(test)]
mod http_session_test;

#[cfg(test)]
#[path = "http_session_live_template_payload_test.rs"]
#[cfg(test)]
mod http_session_live_template_payload_test;

#[cfg(test)]
#[path = "http_session_live_template_response_test.rs"]
#[cfg(test)]
mod http_session_live_template_response_test;

#[path = "http_session/state.rs"]
mod state;

pub(crate) use state::*;
