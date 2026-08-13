//! Coarse supervisor-to-execution-shard control protocol.

#[cfg(test)]
use std::sync::Arc;

/// Maximum serialized bytes carried by one cross-shard route command.
#[cfg(test)]
const MAX_CROSS_SHARD_ENVELOPE_BYTES: usize = 1024 * 1024;

/// Stable identity of one supervised execution shard.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmExecutionShardId(
    /// Validated non-empty shard identity.
    String,
);

impl VmExecutionShardId {
    /// Creates a non-empty execution-shard identity.
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, VmShardControlError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(VmShardControlError::EmptyShardId);
        }
        Ok(Self(value))
    }

    /// Returns the stable shard identity.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Monotonic generation assigned to one admitted shard image.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmShardEpoch(
    /// Validated nonzero generation number.
    u64,
);

impl VmShardEpoch {
    /// Creates a nonzero shard generation.
    pub(crate) fn new(value: u64) -> Result<Self, VmShardControlError> {
        if value == 0 {
            return Err(VmShardControlError::ZeroShardEpoch);
        }
        Ok(Self(value))
    }

    /// Returns the numeric shard generation.
    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Validated identity and descriptor digest of one sealed native image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSealedShardImage {
    /// Stable image identity.
    identity: String,
    /// Digest of the sealed executable descriptor.
    descriptor_digest: [u8; 32],
    /// Ordered continuation identities admitted from the descriptor.
    continuation_ids: Vec<u64>,
}

impl VmSealedShardImage {
    /// Creates sealed-image metadata admitted by the supervisor.
    pub(crate) fn new(
        identity: impl Into<String>,
        descriptor_digest: [u8; 32],
    ) -> Result<Self, VmShardControlError> {
        let identity = identity.into();
        if identity.trim().is_empty() {
            return Err(VmShardControlError::EmptyImageIdentity);
        }
        if descriptor_digest == [0; 32] {
            return Err(VmShardControlError::EmptyImageDigest);
        }
        Ok(Self {
            identity,
            descriptor_digest,
            continuation_ids: Vec::new(),
        })
    }

    /// Attaches canonical continuation identities from the admitted descriptor.
    pub(crate) fn with_continuations(mut self, mut continuation_ids: Vec<u64>) -> Self {
        continuation_ids.sort_unstable();
        continuation_ids.dedup();
        self.continuation_ids = continuation_ids;
        self
    }

    /// Returns the stable sealed-image identity.
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    /// Returns the executable descriptor digest.
    pub(crate) const fn descriptor_digest(&self) -> &[u8; 32] {
        &self.descriptor_digest
    }

    /// Returns ordered continuation identities admitted with this image.
    pub(crate) fn continuation_ids(&self) -> &[u64] {
        &self.continuation_ids
    }
}

/// Monotonic identity of one supervisor control request.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmShardControlRequestId(
    /// Validated nonzero request number.
    u64,
);

#[cfg(test)]
impl VmShardControlRequestId {
    /// Creates a nonzero control request identity.
    pub(crate) fn new(value: u64) -> Result<Self, VmShardControlError> {
        if value == 0 {
            return Err(VmShardControlError::ZeroRequestId);
        }
        Ok(Self(value))
    }

    /// Returns the numeric request identity.
    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Exhaustive class of supervisor-to-shard control traffic.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardControlClass {
    /// Admits one sealed native image generation.
    Admission,
    /// Changes shard-level lifecycle intent.
    Lifecycle,
    /// Requests a bounded shard inspection snapshot.
    Inspection,
    /// Routes an opaque envelope between distinct shards.
    CrossShardRouting,
    /// Requests recovery from one failed shard epoch.
    Recovery,
}

#[cfg(test)]
impl VmShardControlClass {
    /// Every class admitted by the coarse protocol.
    pub(crate) const ALL: [Self; 5] = [
        Self::Admission,
        Self::Lifecycle,
        Self::Inspection,
        Self::CrossShardRouting,
        Self::Recovery,
    ];
}

/// Coarse lifecycle intent sent by the supervisor.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardLifecycleDirective {
    /// Starts control-plane admission for a new shard process.
    Start,
    /// Stops accepting new routed work while existing work drains.
    Drain,
    /// Requests orderly shard termination.
    Stop,
}

/// Bounded inspection subject owned by the shard control plane.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardInspectionSubject {
    /// Shard health and progress counters.
    Health,
    /// Process inventory summarized by the shard.
    Processes,
    /// Resource inventory summarized by the shard.
    Resources,
}

/// Typed validation failure for a shard control command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmShardControlError {
    /// A shard identity was empty or whitespace-only.
    EmptyShardId,
    /// A control request used the reserved zero identity.
    #[cfg(test)]
    ZeroRequestId,
    /// Admission omitted the sealed image identity.
    EmptyImageIdentity,
    /// Admission used an all-zero descriptor digest.
    EmptyImageDigest,
    /// A cross-shard route named the same source and destination.
    #[cfg(test)]
    SameShardRoute,
    /// A cross-shard route carried no envelope bytes.
    #[cfg(test)]
    EmptyRouteEnvelope,
    /// A cross-shard envelope exceeded the fixed control-plane bound.
    #[cfg(test)]
    RouteEnvelopeTooLarge {
        /// Received envelope size.
        actual: usize,
        /// Maximum admitted envelope size.
        maximum: usize,
    },
    /// Recovery named the reserved zero epoch.
    #[cfg(test)]
    ZeroRecoveryEpoch,
    /// A shard generation used the reserved zero identity.
    ZeroShardEpoch,
}

/// Closed payload set for one supervisor-to-shard control request.
#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum VmShardControlPayload {
    /// Sealed image metadata to admit before shard readiness.
    Admission {
        /// Validated sealed image metadata.
        image: VmSealedShardImage,
    },
    /// Shard-level lifecycle intent.
    Lifecycle {
        /// Requested lifecycle operation.
        directive: VmShardLifecycleDirective,
    },
    /// Bounded shard inspection request.
    Inspection {
        /// Inspection dataset requested from the shard.
        subject: VmShardInspectionSubject,
    },
    /// Opaque message routed between distinct shards.
    CrossShardRouting {
        /// Shard that accepted the original route.
        source: VmExecutionShardId,
        /// Shard that owns the remote destination.
        destination: VmExecutionShardId,
        /// Bounded serialized cross-shard envelope.
        envelope: Arc<[u8]>,
    },
    /// Recovery request for one failed shard generation.
    Recovery {
        /// Shard whose failed generation is being recovered.
        failed_shard: VmExecutionShardId,
        /// Exact failed generation epoch.
        failed_epoch: u64,
    },
}

/// One validated coarse supervisor-to-shard command.
#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmShardControlCommand {
    /// Correlation identity assigned by the supervisor.
    request_id: VmShardControlRequestId,
    /// One closed coarse control-plane operation.
    payload: VmShardControlPayload,
}

#[cfg(test)]
impl VmShardControlCommand {
    /// Creates sealed-image admission without transporting application calls.
    pub(crate) fn admission(
        request_id: VmShardControlRequestId,
        image_identity: impl Into<String>,
        descriptor_digest: [u8; 32],
    ) -> Result<Self, VmShardControlError> {
        let image = VmSealedShardImage::new(image_identity, descriptor_digest)?;
        Ok(Self {
            request_id,
            payload: VmShardControlPayload::Admission { image },
        })
    }

    /// Creates one shard-level lifecycle directive.
    pub(crate) const fn lifecycle(
        request_id: VmShardControlRequestId,
        directive: VmShardLifecycleDirective,
    ) -> Self {
        Self {
            request_id,
            payload: VmShardControlPayload::Lifecycle { directive },
        }
    }

    /// Creates one bounded shard inspection request.
    pub(crate) const fn inspection(
        request_id: VmShardControlRequestId,
        subject: VmShardInspectionSubject,
    ) -> Self {
        Self {
            request_id,
            payload: VmShardControlPayload::Inspection { subject },
        }
    }

    /// Creates one route between two distinct execution shards.
    pub(crate) fn cross_shard_route(
        request_id: VmShardControlRequestId,
        source: VmExecutionShardId,
        destination: VmExecutionShardId,
        envelope: impl Into<Arc<[u8]>>,
    ) -> Result<Self, VmShardControlError> {
        if source == destination {
            return Err(VmShardControlError::SameShardRoute);
        }
        let envelope = envelope.into();
        if envelope.is_empty() {
            return Err(VmShardControlError::EmptyRouteEnvelope);
        }
        if envelope.len() > MAX_CROSS_SHARD_ENVELOPE_BYTES {
            return Err(VmShardControlError::RouteEnvelopeTooLarge {
                actual: envelope.len(),
                maximum: MAX_CROSS_SHARD_ENVELOPE_BYTES,
            });
        }
        Ok(Self {
            request_id,
            payload: VmShardControlPayload::CrossShardRouting {
                source,
                destination,
                envelope,
            },
        })
    }

    /// Creates one recovery request for an exact failed shard epoch.
    pub(crate) fn recovery(
        request_id: VmShardControlRequestId,
        failed_shard: VmExecutionShardId,
        failed_epoch: u64,
    ) -> Result<Self, VmShardControlError> {
        if failed_epoch == 0 {
            return Err(VmShardControlError::ZeroRecoveryEpoch);
        }
        Ok(Self {
            request_id,
            payload: VmShardControlPayload::Recovery {
                failed_shard,
                failed_epoch,
            },
        })
    }

    /// Returns the request identity used for correlation.
    pub(crate) const fn request_id(&self) -> VmShardControlRequestId {
        self.request_id
    }

    /// Returns the coarse control-plane class of this command.
    pub(crate) const fn class(&self) -> VmShardControlClass {
        match self.payload {
            VmShardControlPayload::Admission { .. } => VmShardControlClass::Admission,
            VmShardControlPayload::Lifecycle { .. } => VmShardControlClass::Lifecycle,
            VmShardControlPayload::Inspection { .. } => VmShardControlClass::Inspection,
            VmShardControlPayload::CrossShardRouting { .. } => {
                VmShardControlClass::CrossShardRouting
            }
            VmShardControlPayload::Recovery { .. } => VmShardControlClass::Recovery,
        }
    }
}

#[cfg(test)]
#[path = "execution_shard_protocol_test.rs"]
#[cfg(test)]
mod execution_shard_protocol_test;
