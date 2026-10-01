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

/// Package-owned application context over the remaining VM storage adapter.
pub(crate) type VmHttpSessionService =
    terlan_http_native::session_service::SessionService<VmHttpSessionRuntime>;

#[path = "http_session/diagnostics.rs"]
mod diagnostics;

#[path = "http_session/package_storage.rs"]
mod package_storage;

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
