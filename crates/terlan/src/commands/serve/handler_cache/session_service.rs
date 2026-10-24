//! Host application identity and scheduler wiring for package-owned sessions.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use crate::runtime::vm::actor_state::VmActorStateStore;
use crate::runtime::vm::protocol_task_executor::VmProtocolMaintenance;
use terlan_http_native::session_service::{
    ApplicationSessions, SessionService, SESSION_MAINTENANCE_INTERVAL,
};
use terlan_http_native::session_store::SessionStore;

type HttpSessionService = SessionService<SessionStore<VmActorStateStore>>;

static HTTP_SESSION_SERVICES: OnceLock<
    ApplicationSessions<PathBuf, SessionStore<VmActorStateStore>>,
> = OnceLock::new();

fn session_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "load VM HTTP session service",
        rendered,
    )
}

/// Returns the package-owned session service for one served application.
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
    HTTP_SESSION_SERVICES
        .get_or_init(ApplicationSessions::default)
        .get_or_create(key, || {
            SessionStore::for_application(VmActorStateStore::default())
        })
}

/// The application keeps its session clock across handler image generations.
/// Cleanup is bounded and runs on an existing owner, including while idle.
pub(in crate::commands::serve) fn http_session_maintenance_for(
    web_root: &Path,
) -> Result<VmProtocolMaintenance, BoundaryError> {
    let service = http_session_service_for(web_root)?;
    VmProtocolMaintenance::new(SESSION_MAINTENANCE_INTERVAL, move || {
        service.maintain_application()
    })
}

/// Creates an isolated application context; the serving owner attaches its clock.
#[cfg(test)]
pub(super) fn new_session_service() -> HttpSessionService {
    SessionService::new(SessionStore::for_application(VmActorStateStore::default()))
}
