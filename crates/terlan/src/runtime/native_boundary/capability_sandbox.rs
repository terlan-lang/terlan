//! Portable contract constants for capability-worker sandbox profiles.

/// Stable name of the first Linux bubblewrap sandbox profile.
pub(crate) const LINUX_BWRAP_PROFILE: &str = "linux-bwrap-v1";

/// Operating-system family relevant to capability-worker admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapabilitySandboxHost {
    /// Linux host with the bubblewrap and prlimit backend.
    Linux,
    /// Apple Darwin host requiring a signed App Sandbox helper.
    MacOs,
    /// Windows host requiring an LPAC/AppContainer and Job Object backend.
    Windows,
    /// Host without a declared capability-worker security contract.
    Other,
}

impl CapabilitySandboxHost {
    /// Returns the host family selected by the Rust compilation target.
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        }
    }

    /// Returns a stable diagnostic name for platform admission.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Other => "unsupported",
        }
    }
}

/// Closed set of operating-system sandbox profiles admitted in this release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapabilitySandboxProfile {
    /// Linux bubblewrap namespace and prlimit profile.
    LinuxBwrapV1,
}

impl CapabilitySandboxProfile {
    /// Selects the mandatory profile for the current compilation target.
    pub(crate) fn current() -> Result<Self, String> {
        Self::for_host(CapabilitySandboxHost::current())
    }

    /// Selects a profile only when an equivalent backend is implemented.
    pub(crate) fn for_host(host: CapabilitySandboxHost) -> Result<Self, String> {
        match host {
            CapabilitySandboxHost::Linux => Ok(Self::LinuxBwrapV1),
            CapabilitySandboxHost::MacOs => Err(unsupported_backend(
                host,
                "a signed App Sandbox helper is not packaged",
            )),
            CapabilitySandboxHost::Windows => Err(unsupported_backend(
                host,
                "an LPAC/AppContainer and Job Object helper is not packaged",
            )),
            CapabilitySandboxHost::Other => Err(unsupported_backend(
                host,
                "no capability-worker sandbox contract is declared",
            )),
        }
    }

    /// Returns the stable command-line identity for this profile.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::LinuxBwrapV1 => LINUX_BWRAP_PROFILE,
        }
    }

    /// Returns the host family required by this profile.
    pub(crate) const fn host(self) -> CapabilitySandboxHost {
        match self {
            Self::LinuxBwrapV1 => CapabilitySandboxHost::Linux,
        }
    }
}

/// Builds one stable fail-closed platform admission diagnostic.
fn unsupported_backend(host: CapabilitySandboxHost, reason: &str) -> String {
    format!(
        "error[capability_worker.platform]: external capability workers are unavailable on {}: {reason}",
        host.name()
    )
}

/// Worker-visible private working directory inside the sandbox.
#[cfg(target_os = "linux")]
pub(crate) const SANDBOX_WORK_DIR: &str = "/work";

/// Worker-visible temporary directory inside the sandbox.
#[cfg(target_os = "linux")]
pub(crate) const SANDBOX_TEMP_DIR: &str = "/tmp";

/// Deterministic locale admitted into the scrubbed worker environment.
#[cfg(target_os = "linux")]
pub(crate) const SANDBOX_LOCALE: &str = "C.UTF-8";

/// Hard resource limits shared by the VM launcher and worker attestation.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CapabilitySandboxLimits {
    /// Maximum virtual address space in bytes.
    pub(crate) address_space_bytes: u64,
    /// Maximum CPU time consumed by one worker process in seconds.
    pub(crate) cpu_seconds: u64,
    /// Maximum size of one file created by the worker in bytes.
    pub(crate) file_bytes: u64,
    /// Maximum number of descriptors open in the worker process.
    pub(crate) open_files: u64,
    /// Maximum process count admitted by the operating-system user limit.
    pub(crate) processes: u64,
}

#[cfg(target_os = "linux")]
impl CapabilitySandboxLimits {
    /// Returns the fixed resource envelope for the first Linux profile.
    pub(crate) const fn linux_default() -> Self {
        Self {
            address_space_bytes: 512 * 1024 * 1024,
            cpu_seconds: 60,
            file_bytes: 16 * 1024 * 1024,
            open_files: 64,
            processes: 512,
        }
    }
}

#[cfg(test)]
#[path = "capability_sandbox_test.rs"]
#[cfg(test)]
mod capability_sandbox_test;
