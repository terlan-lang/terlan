//! Signal registration belongs to this executable, never the shared VM library.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Keeps ordinary Unix cancellation observable until the final report is sealed.
pub(super) struct Shutdown {
    flag: Arc<AtomicBool>,
    #[cfg(unix)]
    registrations: Vec<signal_hook::SigId>,
}

impl Shutdown {
    /// Installs only flag-setting handlers; child cleanup runs in normal code.
    pub(super) fn install() -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            let mut shutdown = Self {
                flag: Arc::new(AtomicBool::new(false)),
                registrations: Vec::new(),
            };
            for signal in [
                signal_hook::consts::SIGINT,
                signal_hook::consts::SIGTERM,
                signal_hook::consts::SIGHUP,
            ] {
                shutdown.registrations.push(signal_hook::flag::register(
                    signal,
                    Arc::clone(&shutdown.flag),
                )?);
            }
            Ok(shutdown)
        }
        // Windows console/job cancellation requires its own platform owner.
        #[cfg(not(unix))]
        Ok(Self {
            flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Exposes cancellation without allowing callers to replace the registration.
    pub(super) fn flag(&self) -> &AtomicBool {
        &self.flag
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        #[cfg(unix)]
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod tests;
