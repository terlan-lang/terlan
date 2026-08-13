//! Application-lifetime VM HTTP session service ownership.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};

static HTTP_SESSION_SERVICES: OnceLock<RwLock<HashMap<PathBuf, VmHttpSessionService>>> =
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
) -> Result<VmHttpSessionService, BoundaryError> {
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
    let service = VmHttpSessionService::new(
        VmHttpSessionRuntime::new("terlc-serve", 86_400).map_err(session_error)?,
    );
    let mut services = services
        .write()
        .map_err(|_| session_error("error[serve.session_cache]: session service lock poisoned"))?;
    Ok(services
        .entry(key)
        .or_insert_with(|| service.clone())
        .clone())
}

#[cfg(test)]
pub(super) fn test_session_service() -> Result<VmHttpSessionService, BoundaryError> {
    VmHttpSessionRuntime::new("terlc-serve", 86_400)
        .map(VmHttpSessionService::new)
        .map_err(session_error)
}
