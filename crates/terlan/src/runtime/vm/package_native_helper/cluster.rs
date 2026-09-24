//! Actor-owned cluster descriptors; these services never launch a worker.

use crate::runtime::vm::coordination_membership::VmClusterMembership;
use crate::runtime::vm::coordination_profile::VmCoordinationProfile;
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::{ReplValue, VmRuntimeResult};
use crate::terlan_native_boundary::resource::{ResourceError, ResourceRegistry};

const PROFILE: &str = "std.vm.Cluster.Profile";
const MEMBERSHIP: &str = "std.vm.Cluster.Membership";
const SESSION: &str = "std.vm.Cluster.Session";
const FRAME: &str = "std.vm.Cluster.Frame";

#[path = "cluster_membership.rs"]
mod membership;
#[path = "cluster_session.rs"]
mod session;

/// Payload kinds share one id space so relabelled handles cannot alias another kind.
enum ClusterResource {
    Profile(VmCoordinationProfile),
    Membership(VmClusterMembership),
    Session(Box<session::Session>),
    Frame(session::Frame),
}

#[cfg(test)]
#[path = "cluster_test.rs"]
mod tests;

#[cfg(test)]
#[path = "cluster_session_test.rs"]
mod session_tests;

/// Typed state for the cluster service, owned by the VM execution thread.
#[derive(Default)]
pub(super) struct VmClusterRuntime {
    resources: ResourceRegistry<ClusterResource>,
}

impl VmClusterRuntime {
    /// Runs only implemented operations; unknown cluster calls never escape to helpers.
    pub(super) fn call_with_atoms(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
        admitted_atoms: &[String],
    ) -> VmRuntimeResult<ReplValue> {
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[vm.cluster.arguments]: cluster operations require typed arguments")?;
        match request.operation.as_str() {
            "std.vm.cluster.profile" if args.len() == 6 => {
                let profile = VmCoordinationProfile::new(
                    text(&args[0])?,
                    text(&args[1])?,
                    text(&args[2])?,
                    text(&args[3])?,
                    1,
                    text(&args[4])?,
                    [text(&args[5])?],
                )?;
                self.insert(owner, profile)
            }
            "std.vm.cluster.profile_node_id" if args.len() == 1 => Ok(ReplValue::String(
                self.profile(owner, &args[0])?.node_id().to_string(),
            )),
            "std.vm.cluster.profile_epoch" if args.len() == 1 => {
                let epoch = self.profile(owner, &args[0])?.epoch();
                Ok(ReplValue::Int(i64::try_from(epoch).map_err(|_| {
                    "error[vm.cluster.epoch]: profile epoch exceeds Terlan Int"
                })?))
            }
            "std.vm.cluster.profile_next_epoch" if args.len() == 1 => {
                let next = self.profile(owner, &args[0])?.next_epoch()?;
                self.insert(owner, next)
            }
            operation
                if operation == "std.vm.cluster.membership"
                    || operation.starts_with("std.vm.cluster.membership_") =>
            {
                self.membership_call(owner, operation, args)
            }
            operation => self.session_call(owner, operation, args, admitted_atoms),
        }
    }

    fn insert(&mut self, owner: u64, profile: VmCoordinationProfile) -> VmRuntimeResult<ReplValue> {
        i64::try_from(profile.epoch())
            .map_err(|_| "error[vm.cluster.epoch]: profile epoch exceeds Terlan Int")?;
        self.insert_resource(owner, ClusterResource::Profile(profile))
    }

    fn insert_resource(
        &mut self,
        owner: u64,
        value: ClusterResource,
    ) -> VmRuntimeResult<ReplValue> {
        let kind = match &value {
            ClusterResource::Profile(_) => PROFILE,
            ClusterResource::Membership(_) => MEMBERSHIP,
            ClusterResource::Session(_) => SESSION,
            ClusterResource::Frame(_) => FRAME,
        };
        let handle = self
            .resources
            .insert_for_owner(owner, value)
            .map_err(resource_error)?;
        super::direct_std::native_handle_value(owner, handle, kind)
    }

    fn profile(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&VmCoordinationProfile> {
        match self.resource(owner, value, PROFILE)? {
            ClusterResource::Profile(profile) => Ok(profile),
            _ => Err("error[vm.cluster.kind]: stored resource is not a Profile".into()),
        }
    }

    fn resource(
        &self,
        owner: u64,
        value: &ReplValue,
        expected: &str,
    ) -> VmRuntimeResult<&ClusterResource> {
        let ReplValue::Record { name, fields } = value else {
            return Err("error[vm.cluster.handle]: expected an opaque Cluster resource".into());
        };
        let (handle, kind, claimed_owner) = super::direct_std::native_handle(fields)
            .ok_or("error[vm.cluster.handle]: expected a complete Cluster handle")??;
        if Some(name.as_str()) != expected.rsplit('.').next()
            || kind != expected
            || claimed_owner != owner.to_string()
        {
            return Err(
                "error[vm.cluster.handle]: foreign owner or invalid Cluster identity".into(),
            );
        }
        self.resources
            .get_for_owner(handle, owner)
            .map_err(|error| resource_error(error).into())
    }

    /// Releases only descriptors owned by the actor that completed or failed.
    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
    }
}

fn resource_error(error: ResourceError) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

fn text(value: &ReplValue) -> VmRuntimeResult<&str> {
    match value {
        ReplValue::String(value) => Ok(value),
        ReplValue::StringBytes(value) => std::str::from_utf8(value)
            .map_err(|_| "error[vm.cluster.text]: expected valid UTF-8".into()),
        _ => Err("error[vm.cluster.text]: expected String".into()),
    }
}
