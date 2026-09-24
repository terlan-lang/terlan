//! Validated VM node identity shared by source adapters and coordination.

use std::collections::BTreeSet;

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
    /// Creates a validated coordination profile with deterministic capability ordering.
    pub(crate) fn new(
        app_id: impl Into<String>,
        vm_id: impl Into<String>,
        node_id: impl Into<String>,
        cluster_id: impl Into<String>,
        epoch: u64,
        runtime_version: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, String> {
        let app_id = app_id.into();
        let vm_id = vm_id.into();
        let node_id = node_id.into();
        let cluster_id = cluster_id.into();
        let runtime_version = runtime_version.into();
        let capabilities = capabilities
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<String>>();

        for (field, value) in [
            ("app_id", app_id.as_str()),
            ("vm_id", vm_id.as_str()),
            ("node_id", node_id.as_str()),
            ("cluster_id", cluster_id.as_str()),
            ("runtime_version", runtime_version.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "error[vm_coordination_profile]: `{field}` must not be empty"
                ));
            }
        }
        if epoch == 0 {
            return Err("error[vm_coordination_profile]: `epoch` must be non-zero".to_string());
        }
        if capabilities.iter().any(|value| value.trim().is_empty()) {
            return Err(
                "error[vm_coordination_profile]: capability names must not be empty".to_string(),
            );
        }

        Ok(Self {
            app_id,
            vm_id,
            node_id,
            cluster_id,
            epoch,
            runtime_version,
            capabilities,
        })
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

    /// Returns this profile advanced to the next restart incarnation.
    pub(crate) fn next_epoch(&self) -> Result<Self, String> {
        let epoch = self.epoch.checked_add(1).ok_or_else(|| {
            "error[vm_coordination_profile]: profile epoch cannot advance beyond UInt64".to_string()
        })?;
        Self::new(
            self.app_id.clone(),
            self.vm_id.clone(),
            self.node_id.clone(),
            self.cluster_id.clone(),
            epoch,
            self.runtime_version.clone(),
            self.capabilities.iter().cloned(),
        )
    }
}
