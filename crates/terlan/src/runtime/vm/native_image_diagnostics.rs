//! Deterministic diagnostics for one admitted native image generation.

use std::collections::BTreeMap;

use serde::Serialize;

const NATIVE_IMAGE_DIAGNOSTIC_SCHEMA: &str = "terlan.vm.native-image-diagnostic.v1";

/// Runtime reference classes that can keep an admitted image generation alive.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum VmNativeGenerationReferenceClass {
    /// Generated frames owned by live actors.
    NativeFrame,
    /// Generated continuations parked across scheduler turns.
    ParkedContinuation,
    /// Detached actor envelopes retaining generation-owned executable state.
    ActorTransfer,
    /// Managed actor heaps whose layouts belong to the generation.
    ActorHeap,
    /// Mailbox values encoded with generation-owned layouts.
    MailboxFragment,
    /// Active timers capable of resuming generation-owned work.
    Timer,
    /// Native resources owned by actors in the generation.
    Resource,
    /// Asynchronous capability callbacks awaiting completion.
    AsyncCapabilityCallback,
    /// Debugger state pinned to generation source metadata.
    Debugger,
    /// Crash metadata retaining generation-owned stack information.
    CrashMetadata,
}

impl VmNativeGenerationReferenceClass {
    /// Returns the stable diagnostic name of this reference class.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::NativeFrame => "native_frames",
            Self::ParkedContinuation => "parked_continuations",
            Self::ActorTransfer => "actor_transfers",
            Self::ActorHeap => "actor_heaps",
            Self::MailboxFragment => "mailbox_fragments",
            Self::Timer => "timers",
            Self::Resource => "resources",
            Self::AsyncCapabilityCallback => "async_capability_callbacks",
            Self::Debugger => "debugger_pins",
            Self::CrashMetadata => "crash_metadata_pins",
        }
    }
}

/// One nonzero generation-reference count in deterministic class order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmNativeGenerationReferenceRecord {
    /// Stable runtime reference class name.
    pub(crate) class: &'static str,
    /// Number of retained references in this class.
    pub(crate) count: usize,
}

/// Complete reference proof used before an image generation is unloaded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmNativeGenerationReferenceSnapshot {
    /// Reference counts indexed by their canonical runtime class.
    counts: BTreeMap<VmNativeGenerationReferenceClass, usize>,
}

impl VmNativeGenerationReferenceSnapshot {
    /// Creates an empty generation-reference proof.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records one class count, omitting zero rows from diagnostics.
    pub(crate) fn record(&mut self, class: VmNativeGenerationReferenceClass, count: usize) {
        if count == 0 {
            self.counts.remove(&class);
        } else {
            self.counts.insert(class, count);
        }
    }

    /// Adds externally tracked pins to one class with saturating diagnostics.
    pub(crate) fn add(&mut self, class: VmNativeGenerationReferenceClass, count: usize) {
        if count == 0 {
            return;
        }
        let next = self.count(class).saturating_add(count);
        self.counts.insert(class, next);
    }

    /// Returns the reference count for one class.
    pub(crate) fn count(&self, class: VmNativeGenerationReferenceClass) -> usize {
        self.counts.get(&class).copied().unwrap_or(0)
    }

    /// Returns the total retained references across every class.
    pub(crate) fn total(&self) -> usize {
        self.counts.values().copied().sum()
    }

    /// Returns whether no runtime state can reach this generation.
    pub(crate) fn is_quiescent(&self) -> bool {
        self.counts.is_empty()
    }

    /// Returns deterministic structured nonzero reference counts.
    pub(crate) fn records(&self) -> Vec<VmNativeGenerationReferenceRecord> {
        self.counts
            .iter()
            .map(|(class, count)| VmNativeGenerationReferenceRecord {
                class: class.name(),
                count: *count,
            })
            .collect()
    }

    /// Renders deterministic nonzero reference counts for diagnostics.
    pub(crate) fn render_pending(&self) -> String {
        if self.is_quiescent() {
            return "none".to_string();
        }
        self.counts
            .iter()
            .map(|(class, count)| format!("{}={count}", class.name()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Immutable support and crash metadata for one admitted native generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmNativeImageDiagnosticMetadata {
    /// Versioned diagnostic schema identity.
    pub(crate) schema: &'static str,
    /// Compiler/build/package/module identity admitted from the descriptor.
    pub(crate) image_identity: String,
    /// Lowercase SHA-256 digest of the admitted executable descriptor.
    pub(crate) descriptor_digest: String,
    /// Ordered compiler-generated continuation identities.
    pub(crate) continuation_ids: Vec<u64>,
    /// Monotonic execution-shard generation assigned at admission.
    pub(crate) generation_epoch: u64,
    /// Total references retaining this generation.
    pub(crate) generation_reference_total: usize,
    /// Whether the generation is safe to unload at capture time.
    pub(crate) generation_quiescent: bool,
    /// Deterministic nonzero generation-reference counts.
    pub(crate) generation_references: Vec<VmNativeGenerationReferenceRecord>,
}

impl VmNativeImageDiagnosticMetadata {
    /// Builds validated metadata without retaining code, paths, terms, or stack addresses.
    pub(crate) fn new(
        image_identity: impl Into<String>,
        descriptor_digest: [u8; 32],
        mut continuation_ids: Vec<u64>,
        generation_epoch: u64,
        references: &VmNativeGenerationReferenceSnapshot,
    ) -> Result<Self, String> {
        let image_identity = image_identity.into();
        if image_identity.trim().is_empty() {
            return Err("native image diagnostic identity cannot be empty".to_string());
        }
        if descriptor_digest == [0; 32] {
            return Err("native image diagnostic descriptor digest cannot be empty".to_string());
        }
        if generation_epoch == 0 {
            return Err("native image diagnostic generation must be nonzero".to_string());
        }
        continuation_ids.sort_unstable();
        continuation_ids.dedup();
        Ok(Self {
            schema: NATIVE_IMAGE_DIAGNOSTIC_SCHEMA,
            image_identity,
            descriptor_digest: hex_bytes(&descriptor_digest),
            continuation_ids,
            generation_epoch,
            generation_reference_total: references.total(),
            generation_quiescent: references.is_quiescent(),
            generation_references: references.records(),
        })
    }
}

/// Encodes bytes as canonical lowercase hexadecimal text.
fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
#[path = "pure_native/generation_lifetime_test.rs"]
#[cfg(test)]
mod generation_lifetime_test;

#[cfg(test)]
#[path = "native_image_diagnostics_test.rs"]
#[cfg(test)]
mod native_image_diagnostics_test;
