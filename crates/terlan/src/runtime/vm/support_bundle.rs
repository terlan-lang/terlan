use super::multicore_replay::VmMulticoreReplayEvidence;
use super::native_image_diagnostics::VmNativeImageDiagnosticMetadata;
#[cfg(test)]
use super::process::VmProcessId;

use serde::Serialize;

const NATIVE_SUPPORT_BUNDLE_SCHEMA: &str = "terlan.vm.native-support-bundle.v1";

#[cfg(test)]
#[path = "support_bundle_test.rs"]
#[cfg(test)]
mod support_bundle_test;

/// Deterministic support bundle for one admitted native image generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmNativeSupportBundle {
    /// Versioned support-bundle schema identity.
    pub(crate) schema: &'static str,
    /// Structural admitted-image and generation-lifetime metadata.
    pub(crate) native_image: VmNativeImageDiagnosticMetadata,
    /// Optional bounded evidence from a live multicore runtime generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) multicore_replay: Option<VmMulticoreReplayEvidence>,
}

impl VmNativeSupportBundle {
    /// Captures one immutable native image record without executable code.
    pub(crate) fn new(native_image: VmNativeImageDiagnosticMetadata) -> Self {
        Self {
            schema: NATIVE_SUPPORT_BUNDLE_SCHEMA,
            native_image,
            multicore_replay: None,
        }
    }

    /// Attaches validated live scheduler evidence without changing image data.
    #[cfg(test)]
    pub(crate) fn with_multicore_replay(
        native_image: VmNativeImageDiagnosticMetadata,
        multicore_replay: VmMulticoreReplayEvidence,
    ) -> Self {
        Self {
            schema: NATIVE_SUPPORT_BUNDLE_SCHEMA,
            native_image,
            multicore_replay: Some(multicore_replay),
        }
    }

    /// Serializes the support bundle deterministically as pretty JSON.
    pub(crate) fn serialized_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("failed to serialize native support bundle: {error}"))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// VM-owned I/O resource identity captured in a support bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSupportBundleReplayResource {
    pub(crate) kind: VmSupportBundleReplayResourceKind,
    pub(crate) handle: String,
}

#[cfg(test)]
impl VmSupportBundleReplayResource {
    /// Creates replayable resource identity metadata.
    pub(crate) fn new(
        kind: VmSupportBundleReplayResourceKind,
        handle: impl Into<String>,
    ) -> Result<Self, String> {
        let handle = handle.into();
        if handle.trim().is_empty() {
            return Err("VM support-bundle resource handle cannot be empty".to_string());
        }
        Ok(Self { kind, handle })
    }
}

/// Stable VM resource categories supported by I/O replay metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmSupportBundleReplayResourceKind {
    TcpStream,
    UdpSocket,
    PackageDownload,
    Timer,
    HttpHandler,
    AcmeWorker,
    WebSocket,
    TlsConnection,
}

/// Optional source identity captured beside an I/O replay step.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSupportBundleReplaySource {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

#[cfg(test)]
impl VmSupportBundleReplaySource {
    /// Creates source identity metadata for a replay step.
    pub(crate) fn new(
        module: impl Into<String>,
        function: impl Into<String>,
        line: u32,
        column: u32,
    ) -> Result<Self, String> {
        let module = module.into();
        let function = function.into();
        if module.trim().is_empty() {
            return Err("VM support-bundle source module cannot be empty".to_string());
        }
        if function.trim().is_empty() {
            return Err("VM support-bundle source function cannot be empty".to_string());
        }
        if line == 0 || column == 0 {
            return Err("VM support-bundle source position must be one-based".to_string());
        }
        Ok(Self {
            module,
            function,
            line,
            column,
        })
    }
}

/// One deterministic VM I/O replay step.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSupportBundleReplayStep {
    pub(crate) sequence: u64,
    pub(crate) process: VmProcessId,
    pub(crate) resource: VmSupportBundleReplayResource,
    pub(crate) operation: String,
    pub(crate) outcome: String,
    pub(crate) source: Option<VmSupportBundleReplaySource>,
}

/// Expected replay shape for one I/O step.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSupportBundleReplayExpectation {
    pub(crate) sequence: u64,
    pub(crate) process: VmProcessId,
    pub(crate) resource: VmSupportBundleReplayResource,
    pub(crate) operation: String,
}

#[cfg(test)]
impl VmSupportBundleReplayExpectation {
    /// Creates an expected replay step identity.
    pub(crate) fn new(
        sequence: u64,
        process: VmProcessId,
        resource: VmSupportBundleReplayResource,
        operation: impl Into<String>,
    ) -> Result<Self, String> {
        let operation = operation.into();
        if sequence == 0 {
            return Err("VM support-bundle replay sequence must be one-based".to_string());
        }
        if operation.trim().is_empty() {
            return Err("VM support-bundle replay operation cannot be empty".to_string());
        }
        Ok(Self {
            sequence,
            process,
            resource,
            operation,
        })
    }
}

/// Complete support-bundle replay metadata snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSupportBundleReplayMetadata {
    /// Admitted native generation associated with these replay steps.
    pub(crate) native_image: Option<VmNativeImageDiagnosticMetadata>,
    pub(crate) scheduler_seed: u64,
    pub(crate) steps: Vec<VmSupportBundleReplayStep>,
    pub(crate) finished: bool,
}

/// VM-owned support-bundle replay metadata recorder.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VmSupportBundleReplayRecorder {
    /// Native generation bound before publication.
    native_image: Option<VmNativeImageDiagnosticMetadata>,
    scheduler_seed: u64,
    next_sequence: u64,
    steps: Vec<VmSupportBundleReplayStep>,
    finished: bool,
}

#[cfg(test)]
impl VmSupportBundleReplayRecorder {
    /// Creates an empty replay recorder for one scheduler seed.
    pub(crate) fn new(scheduler_seed: u64) -> Self {
        Self {
            native_image: None,
            scheduler_seed,
            next_sequence: 1,
            steps: Vec::new(),
            finished: false,
        }
    }

    /// Binds exactly one admitted native generation to this support bundle.
    pub(crate) fn bind_native_image(
        &mut self,
        native_image: VmNativeImageDiagnosticMetadata,
    ) -> Result<(), String> {
        if self.finished {
            return Err("VM support-bundle replay metadata is finished".to_string());
        }
        if self.native_image.is_some() {
            return Err("VM support-bundle native image is already bound".to_string());
        }
        self.native_image = Some(native_image);
        Ok(())
    }

    /// Records one replayable I/O step without source metadata.
    pub(crate) fn record_io_step(
        &mut self,
        process: VmProcessId,
        resource: VmSupportBundleReplayResource,
        operation: impl Into<String>,
        outcome: impl Into<String>,
    ) -> Result<VmSupportBundleReplayStep, String> {
        self.record_io_step_with_source(process, resource, operation, outcome, None)
    }

    /// Records one replayable I/O step with optional source metadata.
    pub(crate) fn record_io_step_with_source(
        &mut self,
        process: VmProcessId,
        resource: VmSupportBundleReplayResource,
        operation: impl Into<String>,
        outcome: impl Into<String>,
        source: Option<VmSupportBundleReplaySource>,
    ) -> Result<VmSupportBundleReplayStep, String> {
        if self.finished {
            return Err("VM support-bundle replay metadata is finished".to_string());
        }
        let operation = operation.into();
        if operation.trim().is_empty() {
            return Err("VM support-bundle replay operation cannot be empty".to_string());
        }
        let outcome = outcome.into();
        if outcome.trim().is_empty() {
            return Err("VM support-bundle replay outcome cannot be empty".to_string());
        }
        let step = VmSupportBundleReplayStep {
            sequence: self.next_sequence,
            process,
            resource,
            operation,
            outcome,
            source,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.steps.push(step.clone());
        Ok(step)
    }

    /// Marks the metadata finished and returns an immutable snapshot.
    pub(crate) fn finish_bundle(&mut self) -> VmSupportBundleReplayMetadata {
        self.finished = true;
        self.metadata()
    }

    /// Returns all replay steps after the provided sequence.
    pub(crate) fn replay_steps_after(&self, sequence: u64) -> Vec<VmSupportBundleReplayStep> {
        self.steps
            .iter()
            .filter(|step| step.sequence > sequence)
            .cloned()
            .collect()
    }

    /// Verifies and returns one replay step by stable identity.
    pub(crate) fn verify_replay_step(
        &self,
        expected: &VmSupportBundleReplayExpectation,
    ) -> Result<&VmSupportBundleReplayStep, String> {
        let step = self
            .steps
            .iter()
            .find(|step| step.sequence == expected.sequence)
            .ok_or_else(|| {
                format!(
                    "VM support-bundle replay step {} was not found",
                    expected.sequence
                )
            })?;
        if step.process != expected.process
            || step.resource != expected.resource
            || step.operation != expected.operation
        {
            return Err(format!(
                "VM support-bundle replay step {} did not match expected I/O identity",
                expected.sequence
            ));
        }
        Ok(step)
    }

    /// Returns a replay metadata snapshot without closing the recorder.
    pub(crate) fn metadata(&self) -> VmSupportBundleReplayMetadata {
        VmSupportBundleReplayMetadata {
            native_image: self.native_image.clone(),
            scheduler_seed: self.scheduler_seed,
            steps: self.steps.clone(),
            finished: self.finished,
        }
    }
}
