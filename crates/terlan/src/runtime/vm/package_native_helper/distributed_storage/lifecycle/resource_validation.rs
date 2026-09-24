//! Supervisor-pinned database resources, checked asynchronously without granting name authority.

use super::*;
use std::collections::BTreeSet;

const MAX_RESOURCES: usize = 16;
const MAX_NAME_BYTES: usize = 67;
/// Reserves bounded registration keys, identities, index metadata, and an in-flight probe set.
pub(super) const RESERVED_BYTES: usize = 8192;

struct Binding {
    name: String,
    backend: String,
    identity: [u8; 32],
}

/// Immutable expectations remain in the VM; workers receive only ordinary identity queries.
pub(in super::super::super) struct Probe {
    adapter: NativeBoundaryHandle,
    bindings: Vec<Binding>,
    next: usize,
    register: bool,
    sequence: i64,
    mode: &'static str,
}

enum Plan {
    Local(Resource),
    Probe(Probe),
}

pub(super) fn is_operation(operation: &str) -> bool {
    matches!(
        operation,
        "require_resource_handle_validation"
            | "resource_handle_validation_proof"
            | "register_resource_handle"
            | "validate_resource_handles"
    )
}

impl Probe {
    fn request(self) -> Request {
        Request {
            backend: self.bindings[self.next].backend.clone(),
            operation: "runtime.storage.open",
            arguments: Vec::new(),
            pending: Pending {
                adapter: self.adapter,
                operation: Operation::Resources(self),
                name: "resource_handle_validation",
            },
        }
    }
}

impl VmDistributedStorageRuntime {
    pub(super) fn prepare_resources(
        &self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<Option<Request>> {
        Ok(match self.resource_plan(owner, operation, args)? {
            Plan::Local(_) => None,
            Plan::Probe(probe) => Some(probe.request()),
        })
    }

    pub(super) fn resource_validation_call(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<Option<ReplValue>> {
        if !is_operation(operation) {
            return Ok(None);
        }
        match self.resource_plan(owner, operation, args)? {
            Plan::Local(resource) => {
                let empty_success = operation == "validate_resource_handles"
                    && matches!(&resource, Resource::Outcome(outcome) if outcome.kind == "resource_handles_validated");
                let value = self.insert(owner, resource)?;
                if empty_success {
                    let handle = self.handle(owner, &args[0], "Adapter")?;
                    let Resource::Adapter(adapter) = self.resources.get_mut_for_owner(handle, owner).map_err(resource_error)? else { unreachable!("checked kind") };
                    adapter.resource_validation = (0, adapter.sequence);
                }
                Ok(Some(value))
            }
            Plan::Probe(_) => Err("error[vm.distributed_storage.dispatch]: resource validation requires the asynchronous owner dispatcher".into()),
        }
    }

    fn resource_plan(
        &self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<Plan> {
        let expected = if matches!(
            operation,
            "register_resource_handle" | "validate_resource_handles"
        ) {
            2
        } else {
            1
        };
        if args.len() != expected {
            return Err(
                "error[vm.distributed_storage.arguments]: invalid resource validation arity".into(),
            );
        }
        let handle = self.handle(owner, &args[0], "Adapter")?;
        let Resource::Adapter(adapter) = self.resource(owner, &args[0], "Adapter")? else {
            unreachable!("checked kind")
        };
        let supported = self.bindings.values().any(Option::is_some);
        if !adapter.open || !adapter.policy.available || !supported {
            if operation == "resource_handle_validation_proof" {
                return Err("error[vm.distributed_storage.proof]: resource validation requires an open adapter and supervisor-pinned resource bindings".into());
            }
            let kind = if adapter.open && adapter.policy.available {
                "unsupported"
            } else {
                "storage_unavailable"
            };
            return Ok(Plan::Local(Resource::Outcome(resource_outcome(
                kind, adapter,
            ))));
        }
        match operation {
            "require_resource_handle_validation" => {
                return Ok(Plan::Local(Resource::Outcome(resource_outcome(
                    "opened", adapter,
                ))))
            }
            "resource_handle_validation_proof" => {
                return Ok(Plan::Local(Resource::Proof(Proof::Resources(
                    adapter.resource_validation.0,
                    adapter.resource_validation.1,
                ))))
            }
            _ => {}
        }
        let register = operation == "register_resource_handle";
        let values = if register {
            std::slice::from_ref(&args[1])
        } else if let ReplValue::List(values) = &args[1] {
            values.as_slice()
        } else {
            return Err(
                "error[vm.distributed_storage.arguments]: expected resource name list".into(),
            );
        };
        if values.len() > MAX_RESOURCES {
            return Ok(Plan::Local(Resource::Outcome(Outcome::failure(
                StorageFailure::Invalid,
                "resource_handle_validation",
                adapter.policy.mode,
                adapter.sequence,
            ))));
        }
        let mut unique = BTreeSet::new();
        for value in values {
            let name = text(value)?;
            if name.len() > MAX_NAME_BYTES || !unique.insert(name) {
                return Ok(Plan::Local(Resource::Outcome(Outcome::failure(
                    StorageFailure::Invalid,
                    "resource_handle_validation",
                    adapter.policy.mode,
                    adapter.sequence,
                ))));
            }
        }
        let mut bindings = Vec::with_capacity(values.len());
        for name in unique {
            let backend = name.strip_prefix("db.").unwrap_or("");
            let Some(Some(identity)) = self.bindings.get(backend) else {
                return Ok(Plan::Local(Resource::Outcome(missing_resource(
                    adapter.policy.mode,
                    adapter.sequence,
                    name,
                ))));
            };
            if !register && adapter.registered_resources.get(name) != Some(identity) {
                return Ok(Plan::Local(Resource::Outcome(missing_resource(
                    adapter.policy.mode,
                    adapter.sequence,
                    name,
                ))));
            }
            bindings.push(Binding {
                name: name.into(),
                backend: backend.into(),
                identity: *identity,
            });
        }
        if bindings.is_empty() {
            // Empty validation is a real successful observation, committed in the local call.
            let mut outcome = resource_outcome("resource_handles_validated", adapter);
            outcome.validated_resources = 0;
            return Ok(Plan::Local(Resource::Outcome(outcome)));
        }
        Ok(Plan::Probe(Probe {
            adapter: handle,
            bindings,
            next: 0,
            register,
            sequence: adapter.sequence,
            mode: adapter.policy.mode,
        }))
    }

    /// Completes one observation or requests the next probe without resuming generated code.
    pub(in super::super::super) fn complete_step(
        &mut self,
        owner: u64,
        pending: Pending,
        reply: NativeBoundaryReplyTerm,
    ) -> VmRuntimeResult<Step> {
        let Operation::Resources(mut probe) = pending.operation else {
            return self.complete(owner, pending, reply).map(Step::Complete);
        };
        self.resources
            .get_for_owner(probe.adapter, owner)
            .map_err(resource_error)?;
        let binding = &probe.bindings[probe.next];
        let NativeBoundaryReplyTerm::Ok(value) = reply else {
            return Err("error[vm.distributed_storage.worker]: resource observation failed; no registration or validation proof was committed".into());
        };
        let failed = if StorageFailure::from_term(&value)
            .map_err(|_| invalid_reply())?
            .is_some()
        {
            true
        } else {
            let Term::Tuple(fields) = &value else {
                return Err(invalid_reply());
            };
            let [Term::Bytes(identity), boundary] = fields.as_slice() else {
                return Err(invalid_reply());
            };
            let identity: [u8; 32] = identity
                .as_slice()
                .try_into()
                .map_err(|_| invalid_reply())?;
            if identity == [0; 32] {
                return Err(invalid_reply());
            }
            completion::status(boundary)?;
            identity != binding.identity
                || self.bindings.get(&binding.backend) != Some(&Some(binding.identity))
        };
        if failed {
            return self
                .insert(
                    owner,
                    Resource::Outcome(missing_resource(probe.mode, probe.sequence, &binding.name)),
                )
                .map(Step::Complete);
        }
        probe.next += 1;
        if probe.next < probe.bindings.len() {
            return Ok(Step::Continue(probe.request()));
        }
        // Reserve the receipt before installing registrations or advancing proof metadata.
        self.budget
            .check(owner, std::mem::size_of::<Resource>() + 64)?;
        let Resource::Adapter(adapter) = self
            .resources
            .get_mut_for_owner(probe.adapter, owner)
            .map_err(resource_error)?
        else {
            return Err(invalid_reply());
        };
        if !adapter.open {
            return Err(
                "error[vm.distributed_storage.closed]: resource validation adapter was closed"
                    .into(),
            );
        }
        let count = if probe.register {
            for binding in probe.bindings {
                adapter
                    .registered_resources
                    .insert(binding.name, binding.identity);
            }
            adapter.registered_resources.len()
        } else {
            probe.bindings.len()
        };
        adapter.resource_validation = (count as i64, probe.sequence);
        let mut outcome = resource_outcome("resource_handles_validated", adapter);
        outcome.sequence = probe.sequence;
        outcome.validated_resources = count as i64;
        self.insert(owner, Resource::Outcome(outcome))
            .map(Step::Complete)
    }
}

fn resource_outcome(kind: &'static str, adapter: &Adapter) -> Outcome {
    Outcome::new(
        kind,
        "resource_handle_validation",
        adapter.policy.mode,
        adapter.sequence,
    )
}

fn missing_resource(mode: &'static str, sequence: i64, name: &str) -> Outcome {
    let mut outcome = Outcome::new(
        "resource_handle_validation_failed",
        "resource_handle_validation",
        mode,
        sequence,
    );
    outcome.missing_resource = name.into();
    outcome.recovery = "recover_resource_handle";
    outcome
}
