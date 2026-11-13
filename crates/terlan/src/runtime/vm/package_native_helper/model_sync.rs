//! VM-owned model-synchronization contract descriptors.

use super::descriptor::VmDescriptorRuntime;
use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeResult};

const KEY: &str = "std.vm.ModelSync.Key";
const VERSION: &str = "std.vm.ModelSync.Version";

/// Model-sync plans are immutable actor-owned descriptors.
#[derive(Default)]
pub(super) struct VmModelSyncRuntime {
    descriptors: VmDescriptorRuntime,
}

impl VmModelSyncRuntime {
    pub(super) fn call(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[vm.model_sync.arguments]: operations require typed arguments")?;
        match (request.operation.as_str(), args) {
            ("std.vm.model_sync.key", [model, id]) => {
                require_text(model)?;
                require_text(id)?;
                self.descriptors.insert(owner, KEY, args.to_vec())
            }
            ("std.vm.model_sync.version", [sequence, writer]) => {
                positive(sequence)?;
                require_text(writer)?;
                self.descriptors.insert(owner, VERSION, args.to_vec())
            }
            ("std.vm.model_sync.initial_version", [writer]) => {
                require_text(writer)?;
                self.descriptors
                    .insert(owner, VERSION, vec![ReplValue::Int(1), writer.clone()])
            }
            ("std.vm.model_sync.next_version", [current, writer]) => {
                require_text(writer)?;
                let sequence = version_sequence(self.descriptors.get(owner, current, VERSION)?)?;
                let next = sequence
                    .checked_add(1)
                    .ok_or("error[vm.model_sync.version]: version sequence exceeds Terlan Int")?;
                self.descriptors
                    .insert(owner, VERSION, vec![ReplValue::Int(next), writer.clone()])
            }
            ("std.vm.model_sync.stale", [expected, current]) => Ok(ReplValue::Bool(
                version_sequence(self.descriptors.get(owner, expected, VERSION)?)?
                    < version_sequence(self.descriptors.get(owner, current, VERSION)?)?,
            )),
            ("std.vm.model_sync.can_apply", [expected, current]) => {
                let expected = self.descriptors.get(owner, expected, VERSION)?;
                let current = self.descriptors.get(owner, current, VERSION)?;
                Ok(ReplValue::Bool(expected.arguments == current.arguments))
            }
            (operation, values) => {
                let Some((kind, arity)) = constructor(operation) else {
                    return unsupported(operation, values.len());
                };
                if values.len() != arity {
                    return unsupported(operation, values.len());
                }
                if operation == "std.vm.model_sync.capability" {
                    require_text(&values[0])?;
                }
                self.descriptors.insert(owner, kind, values.to_vec())
            }
        }
    }

    pub(super) fn close_owner(&mut self, owner: u64) {
        self.descriptors.close_owner(owner);
    }
}

fn constructor(operation: &str) -> Option<(&'static str, usize)> {
    Some(match operation {
        "std.vm.model_sync.write" => ("std.vm.ModelSync.Write", 4),
        "std.vm.model_sync.delete" => ("std.vm.ModelSync.Delete", 3),
        "std.vm.model_sync.conflict" => ("std.vm.ModelSync.Conflict", 3),
        "std.vm.model_sync.capability" => ("std.vm.ModelSync.Capability", 1),
        "std.vm.model_sync.adapter_contract" => ("std.vm.ModelSync.AdapterContract", 3),
        "std.vm.model_sync.persistent_actor_adapter" => {
            ("std.vm.ModelSync.PersistentActorAdapter", 2)
        }
        "std.vm.model_sync.package_store_adapter" => ("std.vm.ModelSync.PackageStoreAdapter", 2),
        "std.vm.model_sync.syncable_model" => ("std.vm.ModelSync.SyncableModel", 3),
        _ => return None,
    })
}

fn version_sequence(descriptor: &super::descriptor::Descriptor) -> VmRuntimeResult<i64> {
    match descriptor.arguments.first() {
        Some(ReplValue::Int(value)) => Ok(*value),
        _ => Err("error[vm.model_sync.version]: invalid version descriptor".into()),
    }
}

fn require_text(value: &ReplValue) -> VmRuntimeResult<&str> {
    match value {
        ReplValue::String(value) if !value.is_empty() => Ok(value),
        ReplValue::StringBytes(value) if !value.is_empty() => std::str::from_utf8(value)
            .map_err(|_| "error[vm.model_sync.text]: expected valid UTF-8".into()),
        _ => Err("error[vm.model_sync.text]: expected non-empty String".into()),
    }
}

fn positive(value: &ReplValue) -> VmRuntimeResult<i64> {
    match value {
        ReplValue::Int(value) if *value > 0 => Ok(*value),
        _ => Err("error[vm.model_sync.version]: expected positive Int".into()),
    }
}

fn unsupported(operation: &str, arity: usize) -> VmRuntimeResult<ReplValue> {
    Err(format!(
        "error[vm.model_sync.operation]: unsupported operation or arity `{operation}`/{arity}"
    )
    .into())
}

#[cfg(test)]
#[path = "model_sync_test.rs"]
mod tests;
