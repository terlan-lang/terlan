//! Application-lifetime VM HTTP session service ownership.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use crate::runtime::vm::actor_state::VmActorStateStore;
use crate::runtime::vm::protocol_task_executor::VmProtocolMaintenance;
use terlan_http_native::session_service::SessionService;
use terlan_http_native::session_store::SessionStore;

type HttpSessionService = SessionService<SessionStore<VmActorStateStore>>;

static HTTP_SESSION_SERVICES: OnceLock<RwLock<HashMap<PathBuf, HttpSessionService>>> =
    OnceLock::new();

fn session_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "load VM HTTP session service",
        rendered,
    )
}

/// Returns the VM-owned session service for one served application.
///
/// Handler generations are disposable code images: watcher invalidation and
/// hot reload may replace them at any time. Session actors instead belong to
/// the application runtime and survive those generation changes.
pub(super) fn http_session_service_for(
    web_root: &Path,
) -> Result<HttpSessionService, BoundaryError> {
    let key = web_root.canonicalize().map_err(|error| {
        session_error(format!(
            "error[serve.session_root]: canonicalize `{}`: {error}",
            web_root.display()
        ))
    })?;
    let services = HTTP_SESSION_SERVICES.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(service) = services
        .read()
        .map_err(|_| session_error("error[serve.session_cache]: session service lock poisoned"))?
        .get(&key)
        .cloned()
    {
        return Ok(service);
    }
    let service = SessionService::new(
        SessionStore::with_defaults(VmActorStateStore::default())
            .map_err(|error| session_error(error.to_string()))?,
    );
    service.start_clock()?;
    let mut services = services
        .write()
        .map_err(|_| session_error("error[serve.session_cache]: session service lock poisoned"))?;
    Ok(services
        .entry(key)
        .or_insert_with(|| service.clone())
        .clone())
}

/// The application keeps its session clock across handler image generations.
/// Cleanup is bounded and runs on an existing owner, including while idle.
pub(in crate::commands::serve) fn http_session_maintenance_for(
    web_root: &Path,
) -> Result<VmProtocolMaintenance, BoundaryError> {
    let service = http_session_service_for(web_root)?;
    VmProtocolMaintenance::new(std::time::Duration::from_secs(1), move || {
        service.maintain(64)
    })
}

#[cfg(test)]
pub(super) fn test_session_service() -> Result<HttpSessionService, BoundaryError> {
    SessionStore::with_defaults(VmActorStateStore::default())
        .map(SessionService::new)
        .map_err(|error| session_error(error.to_string()))
}
