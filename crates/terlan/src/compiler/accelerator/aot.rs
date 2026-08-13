//! Maintained external AOT backend contract for accelerator artifacts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AcceleratorAdmittedToolchain, AcceleratorExecutionDimensions, AcceleratorIrKernel,
    AcceleratorIrModule, AcceleratorIrParameter, AcceleratorIrSource,
};

#[path = "aot/llvm_nvptx.rs"]
mod llvm_nvptx;
pub use llvm_nvptx::LlvmNvptxBackend;

#[path = "aot/synthetic_vector.rs"]
mod synthetic_vector;
pub use synthetic_vector::SyntheticVectorBackend;

/// Stable generated accelerator artifact descriptor schema.
pub const ACCELERATOR_ARTIFACT_SCHEMA: &str = "terlan.accelerator-artifact.v1";

/// One source mapping retained for a generated kernel entrypoint.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorArtifactSource {
    /// Generated entrypoint symbol.
    pub entrypoint: String,
    /// Original Terlan source location.
    pub source: AcceleratorIrSource,
}

/// Complete backend-neutral launch contract for one generated entrypoint.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorArtifactKernel {
    /// Generated entrypoint symbol.
    pub entrypoint: String,
    /// Ordered typed parameters retained from verified AcceleratorIR.
    pub parameters: Vec<AcceleratorIrParameter>,
    /// Statically selected launch dimensions.
    pub dimensions: AcceleratorExecutionDimensions,
    /// Maximum dynamic shared-memory bytes required by the entrypoint.
    pub shared_memory_bytes: u64,
}

/// Immutable descriptor consumed by package loaders and artifact assembly.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorArtifactDescriptor {
    /// Stable descriptor schema.
    pub schema: String,
    /// Generic backend implementation identity.
    pub backend: String,
    /// Accelerator artifact format.
    pub artifact_format: String,
    /// Selected accelerator architecture.
    pub architecture: String,
    /// Normalized source AcceleratorIR hash.
    pub ir_sha256: String,
    /// Explicit admitted toolchain identity.
    pub toolchain: AcceleratorAdmittedToolchain,
    /// Deterministically ordered typed kernel launch contracts.
    pub kernels: Vec<AcceleratorArtifactKernel>,
    /// Source maps retained outside backend diagnostics.
    pub sources: Vec<AcceleratorArtifactSource>,
    /// Artifact file name relative to the descriptor.
    pub artifact: String,
    /// Immutable artifact content identity.
    pub artifact_sha256: String,
    /// Canonical build options included in the cache key.
    pub build_options: BTreeMap<String, String>,
}

/// Projects verified AcceleratorIR kernels into the canonical artifact schema.
pub(super) fn artifact_kernels(kernels: &[AcceleratorIrKernel]) -> Vec<AcceleratorArtifactKernel> {
    let mut kernels = kernels
        .iter()
        .map(|kernel| AcceleratorArtifactKernel {
            entrypoint: kernel.name.clone(),
            parameters: kernel.parameters.clone(),
            dimensions: kernel.dimensions,
            shared_memory_bytes: kernel.shared_memory_bytes,
        })
        .collect::<Vec<_>>();
    kernels.sort_by(|left, right| left.entrypoint.cmp(&right.entrypoint));
    kernels
}

/// Backend compilation request after target and toolchain admission.
#[derive(Debug)]
pub struct AcceleratorAotRequest<'a> {
    /// Verified backend-neutral input module.
    pub ir: &'a AcceleratorIrModule,
    /// Selected accelerator architecture.
    pub architecture: &'a str,
    /// Explicitly admitted maintained toolchain.
    pub toolchain: &'a AcceleratorAdmittedToolchain,
    /// Canonical backend options.
    pub build_options: BTreeMap<String, String>,
    /// Isolated output directory.
    pub output_directory: &'a Path,
}

/// Complete generated AOT artifact and descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorAotArtifact {
    /// Validated descriptor.
    pub descriptor: AcceleratorArtifactDescriptor,
    /// Exact generated artifact bytes.
    pub bytes: Vec<u8>,
    /// Persisted descriptor path.
    pub descriptor_path: PathBuf,
    /// Persisted artifact path.
    pub artifact_path: PathBuf,
    /// Whether the result was restored from the content-addressed cache.
    pub cache_hit: bool,
}

/// Typed failure emitted before package loading.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceleratorAotError {
    /// Input IR did not pass backend-independent verification.
    InvalidIr(String),
    /// Toolchain does not match the selected backend contract.
    Toolchain(String),
    /// Backend cannot lower one admitted IR operation or type.
    Unsupported(String),
    /// External maintained toolchain invocation failed.
    ToolchainFailed(String),
    /// Produced artifact failed structural validation.
    InvalidArtifact(String),
    /// Deterministic artifact I/O failed.
    Io(String),
}

impl AcceleratorAotError {
    /// Returns the stable diagnostic code for this error class.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidIr(_) => "accelerator.aot-invalid-ir",
            Self::Toolchain(_) => "accelerator.aot-toolchain",
            Self::Unsupported(_) => "accelerator.aot-unsupported",
            Self::ToolchainFailed(_) => "accelerator.aot-toolchain-failed",
            Self::InvalidArtifact(_) => "accelerator.aot-invalid-artifact",
            Self::Io(_) => "accelerator.aot-io",
        }
    }
}

impl std::fmt::Display for AcceleratorAotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "error[{}]: {self:?}", self.code())
    }
}

impl std::error::Error for AcceleratorAotError {}

/// Generic compiler-owned AOT backend boundary.
pub trait AcceleratorAotBackend {
    /// Returns the stable backend implementation identity.
    fn identity(&self) -> &'static str;

    /// Compiles one verified module into a validated accelerator artifact.
    fn compile(
        &self,
        request: &AcceleratorAotRequest<'_>,
    ) -> Result<AcceleratorAotArtifact, AcceleratorAotError>;
}

/// Computes a lowercase SHA-256 content identity.
pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "aot_test.rs"]
mod aot_test;
