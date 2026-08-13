//! Public native-adapter ABI policy shared by generated C and C++ packages.

/// Current public adapter ABI admitted by compiler images and runtime loaders.
pub const PUBLIC_ADAPTER_ABI_VERSION: u16 = 1;
/// Maximum encoded command or reply frame accepted by an adapter process.
pub const PUBLIC_ADAPTER_MAX_FRAME_BYTES: usize = 1_048_576;
/// Maximum one-call owned buffer copied through a public adapter.
pub const PUBLIC_ADAPTER_MAX_TRANSFER_BYTES: usize = 16_777_216;

/// Canonical public native-adapter contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeAdapterAbiContract {
    /// Version selected by compiler image admission.
    pub version: u16,
    /// Maximum encoded command or reply frame.
    pub max_frame_bytes: usize,
    /// Maximum owned transfer buffer copied by one call.
    pub max_transfer_bytes: usize,
}

impl NativeAdapterAbiContract {
    /// Returns the single public adapter contract supported by this runtime.
    pub const fn current() -> Self {
        Self {
            version: PUBLIC_ADAPTER_ABI_VERSION,
            max_frame_bytes: PUBLIC_ADAPTER_MAX_FRAME_BYTES,
            max_transfer_bytes: PUBLIC_ADAPTER_MAX_TRANSFER_BYTES,
        }
    }

    /// Produces the cache identity for this contract and one target ABI.
    pub fn cache_identity(
        self,
        target: &str,
        calling_convention: &str,
    ) -> Result<String, BoundaryError> {
        self.cache_identity_untyped(target, calling_convention)
            .map_err(|error| {
                BoundaryError::message(
                    ErrorDomain::NativeBoundary,
                    "build adapter cache identity",
                    error,
                )
            })
    }

    fn cache_identity_untyped(
        self,
        target: &str,
        calling_convention: &str,
    ) -> Result<String, String> {
        validate_identity_text("target", target)?;
        validate_identity_text("calling convention", calling_convention)?;
        Ok(format!(
            "public-adapter-abi-{}:{target}:{calling_convention}:{}:{}:opaque-handles:explicit-context:explicit-capabilities:scoped-resources:status-values:callbacks-forbidden:single-shot",
            self.version, self.max_frame_bytes, self.max_transfer_bytes
        ))
    }

    /// Renders the stable TOML fields embedded in generated adapter metadata.
    pub fn render_metadata(
        self,
        target: &str,
        calling_convention: &str,
    ) -> Result<String, BoundaryError> {
        self.render_metadata_untyped(target, calling_convention)
            .map_err(|error| {
                BoundaryError::message(
                    ErrorDomain::NativeBoundary,
                    "render adapter metadata",
                    error,
                )
            })
    }

    fn render_metadata_untyped(
        self,
        target: &str,
        calling_convention: &str,
    ) -> Result<String, String> {
        self.cache_identity_untyped(target, calling_convention)?;
        Ok(format!(
            "adapter_abi_version = {}\ntarget = {target:?}\ncalling_convention = {calling_convention:?}\nexecution_context = \"explicit\"\nownership = \"opaque_handles\"\ncapability_lifetimes = \"explicit\"\nresource_lifetimes = \"execution_context_scoped\"\nmax_frame_bytes = {}\nmax_transfer_bytes = {}\nstatus_model = \"status_values\"\ncallback_reentrancy = \"forbidden\"\nasync_completion = \"single_shot\"\n",
            self.version, self.max_frame_bytes, self.max_transfer_bytes
        ))
    }
}

/// Resolves the public calling convention for one supported target triple.
pub fn calling_convention_for_target(target: &str) -> Result<&'static str, BoundaryError> {
    calling_convention_for_target_untyped(target).map_err(|error| {
        BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "resolve adapter calling convention",
            error,
        )
    })
}

fn calling_convention_for_target_untyped(target: &str) -> Result<&'static str, String> {
    let architecture = if target.starts_with("x86_64-") {
        "x86_64"
    } else if target.starts_with("aarch64-") {
        "aarch64"
    } else {
        return Err(format!(
            "error[native_adapter.target]: unsupported adapter architecture in `{target}`"
        ));
    };
    if target.contains("windows") {
        Ok("windows_fastcall")
    } else if target.contains("darwin") && architecture == "aarch64" {
        Ok("apple_aarch64")
    } else if target.contains("darwin") || target.contains("linux") {
        Ok("system_v")
    } else {
        Err(format!(
            "error[native_adapter.target]: unsupported adapter operating system in `{target}`"
        ))
    }
}

/// Rejects empty or delimiter-bearing cache identity components.
fn validate_identity_text(kind: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.contains(['\0', '\n', '\r', ':']) {
        return Err(format!(
            "error[native_adapter.identity]: {kind} is not canonical"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "adapter_abi_test.rs"]
#[cfg(test)]
mod adapter_abi_test;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};
