//! Owner-scoped, structurally interned descriptors for VM contract modules.

use super::{ReplValue, VmRuntimeError, VmRuntimeResult};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::resource::{ResourceError, ResourceRegistry};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Descriptor {
    pub(super) kind: String,
    pub(super) arguments: Vec<ReplValue>,
}

#[derive(Clone, Debug)]
struct InternedDescriptor {
    owner: u64,
    handle: NativeBoundaryHandle,
    descriptor: Descriptor,
}

/// Descriptor constructors are deterministic and actor-owned.
#[derive(Default)]
pub(super) struct VmDescriptorRuntime {
    resources: ResourceRegistry<Descriptor>,
    interned: Vec<InternedDescriptor>,
}

impl VmDescriptorRuntime {
    pub(super) fn insert(
        &mut self,
        owner: u64,
        kind: &str,
        arguments: Vec<ReplValue>,
    ) -> VmRuntimeResult<ReplValue> {
        let descriptor = Descriptor {
            kind: kind.to_string(),
            arguments,
        };
        if let Some(existing) = self
            .interned
            .iter()
            .find(|entry| entry.owner == owner && entry.descriptor == descriptor)
        {
            return super::direct_std::native_handle_value(owner, existing.handle, kind);
        }
        let handle = self
            .resources
            .insert_for_owner(owner, descriptor.clone())
            .map_err(resource_error)?;
        self.interned.push(InternedDescriptor {
            owner,
            handle,
            descriptor,
        });
        super::direct_std::native_handle_value(owner, handle, kind)
    }

    pub(super) fn get(
        &self,
        owner: u64,
        value: &ReplValue,
        expected_kind: &str,
    ) -> VmRuntimeResult<&Descriptor> {
        let ReplValue::Record { name, fields } = value else {
            return Err("error[vm.descriptor.handle]: expected an opaque descriptor".into());
        };
        let (handle, kind, claimed_owner) = super::direct_std::native_handle(fields)
            .ok_or("error[vm.descriptor.handle]: incomplete descriptor handle")??;
        if Some(name.as_str()) != expected_kind.rsplit('.').next()
            || kind != expected_kind
            || claimed_owner != owner.to_string()
        {
            return Err("error[vm.descriptor.handle]: foreign owner or invalid identity".into());
        }
        let descriptor = self
            .resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)?;
        if descriptor.kind != expected_kind {
            return Err("error[vm.descriptor.kind]: descriptor kind does not match".into());
        }
        Ok(descriptor)
    }

    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
        self.interned.retain(|entry| entry.owner != owner);
    }
}

fn resource_error(error: ResourceError) -> VmRuntimeError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}
