#![allow(dead_code)]

use std::collections::BTreeSet;

use super::term_format::{TetfDistributionEnvelope, TetfVmRef};
use super::ReplValue;

/// Identity and capability metadata for one Terlan VM instance.
///
/// Inputs:
/// - Application, VM, node, cluster, epoch, runtime version, and capabilities.
///
/// Output:
/// - Stable metadata used by future transports and local multi-VM tests.
///
/// Transformation:
/// - Keeps coordination explicit: two VM instances do not trust or route to
///   each other until cluster/runtime and capability checks pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmCoordinationProfile {
    app_id: String,
    vm_id: String,
    node_id: String,
    cluster_id: String,
    epoch: u64,
    runtime_version: String,
    capabilities: BTreeSet<String>,
}

impl VmCoordinationProfile {
    /// Creates a coordination profile with deterministic capability ordering.
    pub(crate) fn new(
        app_id: impl Into<String>,
        vm_id: impl Into<String>,
        node_id: impl Into<String>,
        cluster_id: impl Into<String>,
        epoch: u64,
        runtime_version: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            vm_id: vm_id.into(),
            node_id: node_id.into(),
            cluster_id: cluster_id.into(),
            epoch,
            runtime_version: runtime_version.into(),
            capabilities: capabilities
                .into_iter()
                .map(Into::into)
                .collect::<BTreeSet<_>>(),
        }
    }

    /// Returns whether this VM can coordinate with a peer at the metadata layer.
    pub(crate) fn can_coordinate_with(&self, peer: &Self) -> bool {
        self.cluster_id == peer.cluster_id && self.runtime_version == peer.runtime_version
    }

    /// Returns whether this VM advertises every required capability.
    pub(crate) fn has_capabilities<'a>(&self, required: impl IntoIterator<Item = &'a str>) -> bool {
        required
            .into_iter()
            .all(|capability| self.capabilities.contains(capability))
    }

    /// Returns this VM's epoch.
    pub(crate) const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns this VM's application id.
    pub(crate) fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Returns this VM's instance id.
    pub(crate) fn vm_id(&self) -> &str {
        &self.vm_id
    }

    /// Returns this VM's node id.
    pub(crate) fn node_id(&self) -> &str {
        &self.node_id
    }
}

/// Monotonic message id allocator for one VM coordination lane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VmMessageIdAllocator {
    next: u64,
}

impl VmMessageIdAllocator {
    /// Allocates the next monotonic message id.
    pub(crate) fn next(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

/// Metadata envelope for a future cross-VM coordination message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmCoordinationEnvelope {
    pub(crate) message_id: u64,
    pub(crate) trace_id: String,
    pub(crate) from_app_id: String,
    pub(crate) from_vm_id: String,
    pub(crate) from_node_id: String,
    pub(crate) to_app_id: String,
    pub(crate) to_vm_id: String,
    pub(crate) to_node_id: String,
    pub(crate) capability: String,
    pub(crate) epoch: u64,
}

impl VmCoordinationEnvelope {
    /// Builds a checked envelope between two compatible VM profiles.
    pub(crate) fn new(
        message_id: u64,
        from: &VmCoordinationProfile,
        to: &VmCoordinationProfile,
        capability: impl Into<String>,
    ) -> Result<Self, String> {
        let capability = capability.into();
        if !from.can_coordinate_with(to) {
            return Err(
                "error[vm_coordination]: incompatible VM coordination profiles".to_string(),
            );
        }
        if !to.has_capabilities([capability.as_str()]) {
            return Err(format!(
                "error[vm_coordination]: target VM `{}` lacks capability `{capability}`",
                to.vm_id()
            ));
        }
        Ok(Self {
            message_id,
            trace_id: format!("trace:{}:{}:{}", from.vm_id(), to.vm_id(), message_id),
            from_app_id: from.app_id().to_string(),
            from_vm_id: from.vm_id().to_string(),
            from_node_id: from.node_id().to_string(),
            to_app_id: to.app_id().to_string(),
            to_vm_id: to.vm_id().to_string(),
            to_node_id: to.node_id().to_string(),
            capability,
            epoch: to.epoch(),
        })
    }

    /// Builds the TETF distribution envelope for this coordination message.
    pub(crate) fn to_tetf_distribution_envelope(
        &self,
        refs: Vec<TetfVmRef>,
        payload: ReplValue,
    ) -> TetfDistributionEnvelope {
        TetfDistributionEnvelope::new(
            self.trace_id.clone(),
            self.from_node_id.clone(),
            self.to_node_id.clone(),
            self.epoch,
            refs,
            payload,
        )
    }
}

#[cfg(test)]
#[path = "coordination_test.rs"]
mod coordination_test;
