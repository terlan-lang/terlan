//! Shared terminal-state ownership for interactive command surfaces.

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

/// RAII guard that restores terminal raw-mode state.
pub(crate) struct RawModeGuard;

impl RawModeGuard {
    /// Enables terminal raw mode and returns its cleanup owner.
    pub(crate) fn enable() -> std::io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
