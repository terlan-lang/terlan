//! Identical production runtime workloads measured across scheduler widths.

use std::num::NonZeroU64;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::commands::serve::handler::HandlerResponse;
use crate::compiler::router::AotRouterPlan;
use crate::runtime::vm::actor::{VmActorReceive, VmActorRuntime};
use crate::runtime::vm::epmd::protocol::Alive2Request;
use crate::runtime::vm::epmd::state::{ConnectionId, ServerOptions, ServerState};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::runtime::vm::supervision::{VmChildSpec, VmSupervisionRestart, VmSupervisionSystem};
use crate::runtime::vm::ReplValue;

use super::super::invocation::AotHandlerInvocationStep;
use super::super::AotHandlerRuntime;
use super::{timing_distribution, RuntimeWorkloadMeasurement};

/// Stable workload names and operation counts included in the benchmark hash.
pub(super) const WORKLOAD_CONTRACT: &str = "\
actor_spawn_exit:128;\
mailbox_round_trip:256;\
timer_delivery:128;\
http_handler_response:32;\
supervision_restart:64;\
epmd_registration_lifecycle:128";

/// Stable workload order repeated for every requested scheduler width.
pub(super) const WORKLOAD_NAMES: [&str; 6] = [
    "actor_spawn_exit",
    "mailbox_round_trip",
    "timer_delivery",
    "http_handler_response",
    "supervision_restart",
    "epmd_registration_lifecycle",
];

const ACTOR_OPERATIONS: usize = 128;
const MAILBOX_OPERATIONS: usize = 256;
const TIMER_OPERATIONS: usize = 128;
const HTTP_OPERATIONS: usize = 32;
const SUPERVISION_OPERATIONS: usize = 64;
const EPMD_OPERATIONS: usize = 128;
const STATE_MACHINE_SCOPE: &str = "independent_vm_state_machine_lanes";
const AOT_OWNER_SCOPE: &str = "generated_aot_fixed_scheduler_owners";

/// Measures every production workload with identical per-lane work.
pub(super) fn measure_runtime_workloads(
    image: &Path,
    router: Option<AotRouterPlan>,
    package_root: &Path,
    samples: usize,
    widths: &[usize],
) -> Result<Vec<RuntimeWorkloadMeasurement>, String> {
    let mut measurements = Vec::with_capacity(widths.len().saturating_mul(6));
    for &width in widths {
        measurements.push(measure_parallel_workload(
            "actor_spawn_exit",
            STATE_MACHINE_SCOPE,
            width,
            samples,
            ACTOR_OPERATIONS,
            actor_spawn_exit_lane,
        )?);
        measurements.push(measure_parallel_workload(
            "mailbox_round_trip",
            STATE_MACHINE_SCOPE,
            width,
            samples,
            MAILBOX_OPERATIONS,
            mailbox_round_trip_lane,
        )?);
        measurements.push(measure_parallel_workload(
            "timer_delivery",
            STATE_MACHINE_SCOPE,
            width,
            samples,
            TIMER_OPERATIONS,
            timer_delivery_lane,
        )?);
        measurements.push(measure_http_workload(
            image,
            router.clone(),
            package_root,
            width,
            samples,
        )?);
        measurements.push(measure_parallel_workload(
            "supervision_restart",
            STATE_MACHINE_SCOPE,
            width,
            samples,
            SUPERVISION_OPERATIONS,
            supervision_restart_lane,
        )?);
        measurements.push(measure_parallel_workload(
            "epmd_registration_lifecycle",
            STATE_MACHINE_SCOPE,
            width,
            samples,
            EPMD_OPERATIONS,
            epmd_registration_lifecycle_lane,
        )?);
    }
    Ok(measurements)
}

/// Measures one independent production state-machine batch per lane.
fn measure_parallel_workload(
    workload: &'static str,
    execution_scope: &'static str,
    width: usize,
    samples: usize,
    operations_per_lane: usize,
    execute_lane: fn(usize, usize) -> Result<(), String>,
) -> Result<RuntimeWorkloadMeasurement, String> {
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        std::thread::scope(|scope| {
            let joins = (0..width)
                .map(|lane| scope.spawn(move || execute_lane(lane, operations_per_lane)))
                .collect::<Vec<_>>();
            for join in joins {
                join.join()
                    .map_err(|_| format!("{workload} lane panicked"))??;
            }
            Ok::<(), String>(())
        })?;
        durations.push(started.elapsed().as_nanos());
    }
    measurement(
        workload,
        execution_scope,
        width,
        samples,
        operations_per_lane,
        durations,
    )
}

/// Measures generated HTTP response execution on the fixed owner generation.
fn measure_http_workload(
    image: &Path,
    router: Option<AotRouterPlan>,
    package_root: &Path,
    width: usize,
    samples: usize,
) -> Result<RuntimeWorkloadMeasurement, String> {
    let runtime = Arc::new(AotHandlerRuntime::load_with_shard_count(
        "app.MulticoreBenchmark".to_string(),
        image,
        router,
        width,
    )?);
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        std::thread::scope(|scope| {
            let joins = (0..width)
                .map(|_| {
                    let runtime = Arc::clone(&runtime);
                    scope.spawn(move || {
                        for _ in 0..HTTP_OPERATIONS {
                            let result = runtime.begin_request_invocation(
                                "app.MulticoreBenchmark",
                                "http",
                                vec![request_value()],
                            )?;
                            let AotHandlerInvocationStep::Complete(value) = result else {
                                return Err(
                                    "multicore HTTP workload parked unexpectedly".to_string()
                                );
                            };
                            let response = HandlerResponse::from_vm_response_with_package_root(
                                &value,
                                package_root,
                            )?;
                            if response.status != 200 || response.body.as_bytes() != b"multicore" {
                                return Err("multicore HTTP workload returned an invalid response"
                                    .to_string());
                            }
                        }
                        Ok::<(), String>(())
                    })
                })
                .collect::<Vec<_>>();
            for join in joins {
                join.join()
                    .map_err(|_| "HTTP workload lane panicked".to_string())??;
            }
            Ok::<(), String>(())
        })?;
        durations.push(started.elapsed().as_nanos());
    }
    measurement(
        "http_handler_response",
        AOT_OWNER_SCOPE,
        width,
        samples,
        HTTP_OPERATIONS,
        durations,
    )
}

/// Builds one validated measurement from a completed width sample.
fn measurement(
    workload: &'static str,
    execution_scope: &'static str,
    width: usize,
    samples: usize,
    operations_per_lane: usize,
    durations: Vec<u128>,
) -> Result<RuntimeWorkloadMeasurement, String> {
    let timing = timing_distribution(&durations)?;
    let operations_per_sample = width.saturating_mul(operations_per_lane);
    let operations_per_second = (operations_per_sample as u128)
        .saturating_mul(1_000_000_000)
        .checked_div(timing.median_ns.max(1))
        .unwrap_or(0);
    Ok(RuntimeWorkloadMeasurement {
        workload,
        execution_scope,
        requested_schedulers: width,
        samples,
        operations_per_lane,
        operations_per_sample,
        operations_per_second,
        timing,
    })
}

/// Executes process creation and terminal cleanup under one scheduler owner.
fn actor_spawn_exit_lane(lane: usize, operations: usize) -> Result<(), String> {
    let topology = VmSchedulerTopology::new(lane.saturating_add(1))?;
    let scheduler = topology
        .schedulers()
        .nth(lane)
        .ok_or_else(|| "actor benchmark scheduler lane is missing".to_string())?;
    let mut runtime = VmActorRuntime::with_scheduler_owner(scheduler.owner_word())?;
    let source = VmProcessSource::new("app.MulticoreBenchmark", "actor", 0);
    for _ in 0..operations {
        let pid = runtime.spawn_root(source.clone());
        runtime.exit_actor(pid, VmExitReason::Normal)?;
        if runtime.is_alive(pid) {
            return Err("actor benchmark retained an exited process".to_string());
        }
    }
    Ok(())
}

/// Executes ordered local mailbox publication and selective receive.
fn mailbox_round_trip_lane(lane: usize, operations: usize) -> Result<(), String> {
    let mut runtime = VmActorRuntime::with_scheduler_owner(
        NonZeroU64::new(lane as u64 + 1).expect("lane owner is nonzero"),
    )?;
    let sender = runtime.spawn_root(VmProcessSource::new("app.MulticoreBenchmark", "sender", 0));
    let recipient = runtime.spawn_root(VmProcessSource::new(
        "app.MulticoreBenchmark",
        "recipient",
        0,
    ));
    for operation in 0..operations {
        let payload = ReplValue::Int(operation as i64);
        runtime.send(sender, recipient, payload.clone())?;
        let VmActorReceive::Message(message) = runtime.receive_next_or_block(recipient)? else {
            return Err("mailbox benchmark failed to receive a published message".to_string());
        };
        if message.payload != payload {
            return Err("mailbox benchmark changed message ordering or payload".to_string());
        }
    }
    Ok(())
}

/// Executes logical timer publication, deadline delivery, and receive.
fn timer_delivery_lane(lane: usize, operations: usize) -> Result<(), String> {
    let mut runtime = VmActorRuntime::with_scheduler_owner(
        NonZeroU64::new(lane as u64 + 1).expect("lane owner is nonzero"),
    )?;
    let sender = runtime.spawn_root(VmProcessSource::new(
        "app.MulticoreBenchmark",
        "timer_sender",
        0,
    ));
    let recipient = runtime.spawn_root(VmProcessSource::new(
        "app.MulticoreBenchmark",
        "timer_recipient",
        0,
    ));
    for operation in 0..operations {
        let now = (operation as u64).saturating_mul(2);
        let payload = ReplValue::Int(operation as i64);
        runtime.send_after(sender, recipient, payload.clone(), now, 1)?;
        runtime.advance_actor_timers(now + 1);
        let VmActorReceive::Message(message) = runtime.receive_next_or_block(recipient)? else {
            return Err("timer benchmark did not deliver its deadline message".to_string());
        };
        if message.payload != payload {
            return Err("timer benchmark changed deadline delivery order".to_string());
        }
    }
    Ok(())
}

/// Executes a repeated one-for-one supervised child restart lifecycle.
fn supervision_restart_lane(lane: usize, operations: usize) -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor(format!("benchmark-{lane}"));
    let mut child = supervision.start_child(
        &mut processes,
        supervisor,
        VmChildSpec::new(
            "worker",
            VmProcessSource::new("app.MulticoreBenchmark", "worker", 0),
            operations as u32 + 1,
        ),
    )?;
    for _ in 0..operations {
        let reason = VmExitReason::Error("benchmark".to_string());
        processes.exit_process(child, reason.clone())?;
        let restart = supervision.restart_child(&mut processes, supervisor, "worker", reason)?;
        let VmSupervisionRestart::Restarted { new_pid, .. } = restart else {
            return Err("supervision benchmark did not restart its child".to_string());
        };
        child = new_pid;
    }
    Ok(())
}

/// Executes EPMD register, lookup, names, and unregister ownership.
fn epmd_registration_lifecycle_lane(lane: usize, operations: usize) -> Result<(), String> {
    let mut state = ServerState::new(ServerOptions::new(4369));
    let name = format!("benchmark-{lane}@host").into_bytes();
    let request = Alive2Request {
        port: 4040 + lane as u16,
        node_type: 77,
        protocol: 0,
        highest_version: 6,
        lowest_version: 5,
        name: name.clone(),
        extra: b"terlan".to_vec(),
    };
    for operation in 0..operations {
        let connection = ConnectionId::new(operation as u64 + 1);
        let registration = state.register_alive2(connection, &request);
        if !registration.registered
            || state.lookup(&name).is_none()
            || !state
                .names_response()
                .windows(name.len())
                .any(|window| window == name)
        {
            return Err("EPMD benchmark registration was not discoverable".to_string());
        }
        if state.unregister_connection(connection).is_none() || state.registered_len() != 0 {
            return Err("EPMD benchmark registration outlived its connection".to_string());
        }
    }
    Ok(())
}

/// Builds one managed request accepted by the generated HTTP handler.
pub(super) fn request_value() -> ReplValue {
    let empty_map = || ReplValue::Map(Vec::new());
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        ReplValue::String("GET".to_string()),
        ReplValue::String("/multicore".to_string()),
        empty_map(),
        ReplValue::String(String::new()),
        ReplValue::String(String::new()),
        empty_map(),
        empty_map(),
        empty_map(),
        ReplValue::Tuple(vec![empty_map(), ReplValue::List(Vec::new())]),
    ])
}
