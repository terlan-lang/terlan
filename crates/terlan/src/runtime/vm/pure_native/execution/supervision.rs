//! Typed source supervision stays on the owning VM shard, never in a helper.

use super::*;
use crate::runtime::vm::supervision::source::{int, record};
use crate::runtime::vm::supervision::{VmSupervisionError, VmSupervisionErrorKind};

impl PureNativeBoundary {
    pub(super) fn resume_supervision(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
        suspension: PureNativeSuspension,
    ) -> Result<PureNativeExecution, String> {
        actors.validate_native_continuation_owner(
            suspension.owner_id(),
            suspension.request_id(),
            suspension.continuation_id(),
        )?;
        let request = self.capability_request_for_actor(context, &suspension)?;
        let operation = request
            .operation
            .strip_prefix("std.vm.supervisor.")
            .ok_or("invalid supervision operation namespace")?;
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("supervision requires typed arguments")?;
        let result = match (operation, args) {
            ("start_child", [policy, spec]) => {
                self.admit_supervised_child(actors, context.actor(), policy, spec, None)
            }
            ("start_persistent_child", [policy, spec, recovery]) => {
                self.admit_supervised_child(actors, context.actor(), policy, spec, Some(recovery))
            }
            _ => actors.supervised_operation(context.actor(), operation, args),
        };
        let value = match result {
            Ok(value) => ReplValue::Record {
                name: "Ok".into(),
                fields: vec![("value".into(), value)],
            },
            Err(message) => ReplValue::Record {
                name: "Err".into(),
                fields: vec![(
                    "reason".into(),
                    record(
                        "SupervisorError",
                        vec![("message", ReplValue::String(message.to_string()))],
                    ),
                )],
            },
        };
        let execution = self.resume_capability_value_for_actor(
            actors,
            context,
            suspension,
            &request.result_type,
            &value,
        )?;
        self.drive_supervised_children(actors, context)?;
        Ok(execution)
    }

    fn admit_supervised_child(
        &self,
        actors: &mut VmActorRuntime,
        owner: VmProcessId,
        policy: &ReplValue,
        spec: &ReplValue,
        recovery: Option<&ReplValue>,
    ) -> Result<ReplValue, VmSupervisionError> {
        let export = self
            .resolve_spawn_export(int(spec, "value", 2)?)
            .map_err(|message| {
                VmSupervisionError::new(VmSupervisionErrorKind::Configuration, message)
            })?;
        if let Some(recovery) = recovery {
            self.resolve_spawn_export(int(recovery, "restore_entry", 4)?)
                .map_err(|message| {
                    VmSupervisionError::new(VmSupervisionErrorKind::Configuration, message)
                })?;
        }
        actors
            .start_supervised_child(
                owner,
                policy,
                spec,
                recovery,
                VmProcessSource::new(&export.module, &export.function, export.arity),
            )
            .map(|pid| ReplValue::Int(pid.as_u64() as i64))
    }

    pub(in crate::runtime::vm::pure_native) fn drive_supervised_children(
        &mut self,
        actors: &mut VmActorRuntime,
        context: &mut PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        loop {
            let start = actors
                .take_supervised_start()
                .map_err(|error| error.to_string())?;
            while let Some(retired) = actors.take_supervised_retired() {
                let mut retired_context = context.reborrow(retired);
                self.release_owner(&mut retired_context)?;
            }
            let Some(start) = start else { break };
            let export = self.resolve_spawn_export(start.entry)?;
            actors.attach_native_spawn_entry(
                start.pid.as_u64(),
                VmProcessSource::new(&export.module, &export.function, export.arity),
            )?;
            self.execute_spawned_child(actors, context, start.pid, &export)?;
        }
        Ok(())
    }
}
