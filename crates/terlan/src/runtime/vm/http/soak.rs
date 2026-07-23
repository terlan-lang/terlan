use std::path::{Path, PathBuf};

use serde::Serialize;

use super::VmHttpTcpServer;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessTable, VmProcessTableMetrics},
    scheduler::VmScheduler,
    tcp::{VmTcpRuntime, VmTcpRuntimeMetrics, VmTcpStream},
    timer::{VmTimerEvent, VmTimerTable},
};

#[path = "soak_pressure.rs"]
mod pressure;
#[path = "soak_stability.rs"]
pub(super) mod stability;
use stability::{
    phase_resources, stability_violations, VmHttpSoakPhaseResources, VmHttpSoakStabilityInput,
    VmHttpSoakStabilityPolicy,
};

const SOAK_ADDRESS: &str = "http-soak.local";
const SOAK_BACKLOG_LIMIT: usize = 16;
const HANDLER_TIMEOUT_TICKS: u64 = 8;
const SOAK_STABILITY_POLICY: VmHttpSoakStabilityPolicy = VmHttpSoakStabilityPolicy {
    max_response_memory_high_water_bytes: 4096,
    max_response_memory_retained_bytes: 0,
    max_final_heap_growth_bytes: 0,
    max_final_resource_handle_growth: 0,
    max_post_warmup_error_rate_bps: 0,
};
const CANONICAL_ROUTES: [(&str, &str); 5] = [
    ("static", "/static"),
    ("json", "/json"),
    ("add", "/add"),
    ("route-param", "/users/42"),
    ("stateful-counter", "/counter"),
];

#[derive(Clone, Copy)]
struct VmHttpSoakProfile {
    name: &'static str,
    cycles: usize,
    adversarial_replays: usize,
}

const SHORT_PROFILE: VmHttpSoakProfile = VmHttpSoakProfile {
    name: "short-deterministic",
    cycles: 8,
    adversarial_replays: 1,
};
const RELEASE_PROFILE: VmHttpSoakProfile = VmHttpSoakProfile {
    name: "release-long",
    cycles: 600,
    adversarial_replays: 3,
};

/// One deterministic resource snapshot retained by the HTTP soak report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmHttpSoakResourceSnapshot {
    pub(crate) process_total: usize,
    pub(crate) process_live: usize,
    pub(crate) process_exited: usize,
    pub(crate) mailbox_messages: usize,
    pub(crate) heap_bytes: usize,
    pub(crate) resource_handles: usize,
    pub(crate) native_boundary_handles: usize,
    pub(crate) listener_total: usize,
    pub(crate) listener_open: usize,
    pub(crate) stream_total: usize,
    pub(crate) stream_open: usize,
    pub(crate) queued_accepts: usize,
    pub(crate) queued_messages: usize,
    pub(crate) queued_bytes: usize,
    pub(crate) waiting_readers: usize,
    pub(crate) waiting_writers: usize,
    pub(crate) active_handlers: usize,
    pub(crate) active_timers: usize,
    pub(crate) active_body_buffers: usize,
    pub(crate) active_body_bytes: usize,
    pub(crate) active_telemetry_spans: usize,
    pub(crate) active_route_contexts: usize,
}

/// Stable terminal evidence for one adversarial HTTP soak phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmHttpSoakTerminal {
    pub(crate) phase: &'static str,
    pub(crate) outcome: &'static str,
    pub(crate) diagnostic: String,
}

/// Persisted deterministic short-profile HTTP soak result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmHttpSoakReport {
    pub(crate) schema: &'static str,
    pub(crate) profile: &'static str,
    pub(crate) canonical_schedule: &'static str,
    pub(crate) cycles: usize,
    pub(crate) canonical_requests: usize,
    pub(crate) adversarial_replays: usize,
    pub(crate) route_mix: Vec<&'static str>,
    pub(crate) stability_policy: VmHttpSoakStabilityPolicy,
    pub(crate) initial_resources: VmHttpSoakResourceSnapshot,
    pub(crate) phase_resources: Vec<VmHttpSoakPhaseResources>,
    pub(crate) accepted_requests: usize,
    pub(crate) completed_requests: usize,
    pub(crate) expected_failures: usize,
    pub(crate) response_errors: usize,
    pub(crate) disconnected_clients: usize,
    pub(crate) backpressure_rejections: usize,
    pub(crate) accept_queue_high_water: usize,
    pub(crate) parked_handlers: usize,
    pub(crate) handler_wakeups: usize,
    pub(crate) handler_wakeup_park_ratio_milli: usize,
    pub(crate) timer_cancellations: usize,
    pub(crate) timer_expirations: usize,
    pub(crate) response_memory_high_water_bytes: usize,
    pub(crate) response_memory_retained_bytes: usize,
    pub(crate) peak_body_buffers: usize,
    pub(crate) peak_body_bytes: usize,
    pub(crate) peak_telemetry_spans: usize,
    pub(crate) peak_route_contexts: usize,
    pub(crate) final_heap_growth_bytes: usize,
    pub(crate) final_resource_handle_growth: usize,
    pub(crate) post_warmup_requests: usize,
    pub(crate) post_warmup_error_rate_bps: usize,
    pub(crate) terminals: Vec<VmHttpSoakTerminal>,
    pub(crate) final_resources: VmHttpSoakResourceSnapshot,
    pub(crate) leak_classifications: Vec<String>,
    pub(crate) steady_state_proven: bool,
    #[serde(skip)]
    pub(crate) report_path: PathBuf,
}

#[derive(Default)]
struct VmHttpSoakCounters {
    expected_failures: usize,
    response_errors: usize,
    disconnected_clients: usize,
    backpressure_rejections: usize,
    accept_queue_high_water: usize,
    parked_handlers: usize,
    handler_wakeups: usize,
    timer_cancellations: usize,
    timer_expirations: usize,
    response_memory_high_water_bytes: usize,
    response_memory_retained_bytes: usize,
    terminals: Vec<VmHttpSoakTerminal>,
    phase_resources: Vec<VmHttpSoakPhaseResources>,
}

struct VmHttpSoakRuntime {
    processes: VmProcessTable,
    tcp: VmTcpRuntime,
    timers: VmTimerTable,
    scheduler: VmScheduler,
    server: VmHttpTcpServer,
    tick: u64,
    stateful_counter: usize,
    counters: VmHttpSoakCounters,
}

/// Runs and persists the canonical deterministic short HTTP soak profile.
pub(crate) fn run_short_http_soak(report_path: &Path) -> Result<VmHttpSoakReport, String> {
    run_http_soak(SHORT_PROFILE, report_path)
}

/// Runs and persists the canonical long HTTP release soak profile.
pub(crate) fn run_release_http_soak(report_path: &Path) -> Result<VmHttpSoakReport, String> {
    run_http_soak(RELEASE_PROFILE, report_path)
}

fn run_http_soak(
    profile: VmHttpSoakProfile,
    report_path: &Path,
) -> Result<VmHttpSoakReport, String> {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen_with_backlog(SOAK_ADDRESS, SOAK_BACKLOG_LIMIT)?;
    let server = VmHttpTcpServer::with_handler_timeout_ticks(
        listener,
        VmProcessSource::new("std.vm.HttpSoak", "handle", 1),
        HANDLER_TIMEOUT_TICKS,
    )?;
    let mut runtime = VmHttpSoakRuntime {
        processes: VmProcessTable::default(),
        tcp,
        timers: VmTimerTable::default(),
        scheduler: VmScheduler::default(),
        server,
        tick: 1,
        stateful_counter: 0,
        counters: VmHttpSoakCounters::default(),
    };
    let initial_resources = runtime.resource_snapshot();

    let canonical_before = runtime.resource_snapshot();
    for _ in 0..profile.cycles {
        for (_, path) in CANONICAL_ROUTES {
            runtime.complete_request(path)?;
        }
    }
    runtime.record_phase("canonical-workload", None, canonical_before);
    for replay in 1..=profile.adversarial_replays {
        runtime.run_adversarial_replay(replay)?;
    }
    runtime.tcp.close_listener(listener)?;

    let final_resources = runtime.resource_snapshot();
    let request_metrics = runtime.server.request_resource_metrics();
    let request_resource_leaks = runtime.server.request_resource_leaks();
    let post_warmup_requests = runtime
        .server
        .accepted_total()
        .saturating_sub(CANONICAL_ROUTES.len());
    let post_warmup_error_rate_bps =
        rate_bps(runtime.counters.response_errors, post_warmup_requests);
    let final_heap_growth_bytes = final_resources
        .heap_bytes
        .saturating_sub(initial_resources.heap_bytes);
    let final_resource_handle_growth = final_resources
        .native_boundary_handles
        .saturating_sub(initial_resources.native_boundary_handles);
    let handler_wakeup_park_ratio_milli = ratio_milli(
        runtime.counters.handler_wakeups,
        runtime.counters.parked_handlers,
    );
    let leak_classifications = stability_violations(VmHttpSoakStabilityInput {
        policy: SOAK_STABILITY_POLICY,
        initial: &initial_resources,
        final_resources: &final_resources,
        response_memory_high_water_bytes: runtime.counters.response_memory_high_water_bytes,
        response_memory_retained_bytes: runtime.counters.response_memory_retained_bytes,
        post_warmup_error_rate_bps,
        last_request_id: request_metrics.last_request_id,
        request_resource_leaks: &request_resource_leaks,
    });
    let report = VmHttpSoakReport {
        schema: "terlan-vm-http-soak-stability-report-v1",
        profile: profile.name,
        canonical_schedule: "benches/http/PROFILE.toml",
        cycles: profile.cycles,
        canonical_requests: profile.cycles * CANONICAL_ROUTES.len(),
        adversarial_replays: profile.adversarial_replays,
        route_mix: CANONICAL_ROUTES.iter().map(|(name, _)| *name).collect(),
        stability_policy: SOAK_STABILITY_POLICY,
        initial_resources,
        phase_resources: runtime.counters.phase_resources,
        accepted_requests: runtime.server.accepted_total(),
        completed_requests: runtime.server.completed_total(),
        expected_failures: runtime.counters.expected_failures,
        response_errors: runtime.counters.response_errors,
        disconnected_clients: runtime.counters.disconnected_clients,
        backpressure_rejections: runtime.counters.backpressure_rejections,
        accept_queue_high_water: runtime.counters.accept_queue_high_water,
        parked_handlers: runtime.counters.parked_handlers,
        handler_wakeups: runtime.counters.handler_wakeups,
        handler_wakeup_park_ratio_milli,
        timer_cancellations: runtime.counters.timer_cancellations,
        timer_expirations: runtime.counters.timer_expirations,
        response_memory_high_water_bytes: runtime.counters.response_memory_high_water_bytes,
        response_memory_retained_bytes: runtime.counters.response_memory_retained_bytes,
        peak_body_buffers: request_metrics.peak_body_buffers,
        peak_body_bytes: request_metrics.peak_body_bytes,
        peak_telemetry_spans: request_metrics.peak_telemetry_spans,
        peak_route_contexts: request_metrics.peak_route_contexts,
        final_heap_growth_bytes,
        final_resource_handle_growth,
        post_warmup_requests,
        post_warmup_error_rate_bps,
        terminals: runtime.counters.terminals,
        steady_state_proven: leak_classifications.is_empty(),
        final_resources,
        leak_classifications,
        report_path: report_path.to_path_buf(),
    };
    write_report(&report)?;
    if !report.steady_state_proven {
        return Err(format!(
            "VM HTTP soak leaked resources: {}",
            report.leak_classifications.join(", ")
        ));
    }
    Ok(report)
}

impl VmHttpSoakRuntime {
    fn run_adversarial_replay(&mut self, replay: usize) -> Result<(), String> {
        self.capture_phase("route-miss", replay, |runtime| {
            runtime.complete_request("/missing")
        })?;
        self.counters.terminals.push(VmHttpSoakTerminal {
            phase: "route-miss",
            outcome: "completed",
            diagnostic: "status=404 shutdown_phase=adversarial_replay".to_string(),
        });
        self.capture_phase("slow-client-write", replay, Self::slow_client_write)?;
        self.capture_phase("cancellation-burst", replay, Self::cancel_parked_handler)?;
        self.capture_phase("request-deadline", replay, Self::expire_parked_handler)?;
        self.capture_phase("malformed-request", replay, |runtime| {
            runtime.reject_request(
                "malformed-request",
                b"BROKEN\r\n\r\n".to_vec(),
                false,
                "failed to parse VM HTTP request: invalid token",
            )
        })?;
        self.capture_phase("oversized-body", replay, |runtime| {
            runtime.reject_request(
                "oversized-body",
                b"POST /add HTTP/1.1\r\nHost: http-soak.local\r\nContent-Length: 1048577\r\n\r\n"
                    .to_vec(),
                false,
                "VM HTTP request exceeded 1 MiB body limit",
            )
        })?;
        self.capture_phase("half-closed-request", replay, |runtime| {
            runtime.reject_request(
                "half-closed-request",
                b"POST /add HTTP/1.1\r\nHost: http-soak.local\r\nContent-Length: 8\r\n\r\nshort"
                    .to_vec(),
                true,
                "VM HTTP request body ended early",
            )
        })?;
        self.capture_phase("client-disconnect-storm", replay, Self::disconnect_storm)?;
        self.capture_phase(
            "accept-queue-saturation",
            replay,
            Self::saturate_accept_queue,
        )
    }

    fn complete_request(&mut self, path: &str) -> Result<(), String> {
        let client = self.tcp.connect(SOAK_ADDRESS, format!("client:{path}"))?;
        let (method, body) = if path == "/add" {
            ("POST", "1,2")
        } else {
            ("GET", "")
        };
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {SOAK_ADDRESS}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        self.tcp.send(client, request.into_bytes())?;
        let tick = self.next_tick();
        let result = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            tick,
            |request| soak_response(request, &mut self.stateful_counter),
        )?;
        self.record_poll(&result);
        if result.http.completed != 1 {
            return Err(format!(
                "VM HTTP soak `{path}` completed {} handlers instead of 1",
                result.http.completed
            ));
        }
        self.record_response_memory();
        drain_client(&mut self.tcp, client)?;
        self.tcp.close_stream(client)
    }

    fn slow_client_write(&mut self) -> Result<(), String> {
        let client = self.tcp.connect(SOAK_ADDRESS, "slow-client")?;
        let first_tick = self.next_tick();
        let first = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            first_tick,
            |_request| Err("slow client reached handler before request completion".to_string()),
        )?;
        self.record_poll(&first);
        let process = self.active_handler("slow-client-write")?;
        let (_, wakeups) = self
            .tcp
            .send_with_wakeups(client, b"GET /static HTTP/1.1\r\n".to_vec())?;
        self.counters.handler_wakeups += wakeups.len();
        if self.processes.get(process).is_none() {
            return Err("VM HTTP slow-client handler disappeared".to_string());
        }
        self.processes
            .with_process_control_mutator(process, |actor| actor.wake())?;
        let partial_tick = self.next_tick();
        let partial = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            partial_tick,
            |_request| Err("slow client partial request reached handler".to_string()),
        )?;
        self.record_poll(&partial);
        let (_, wakeups) = self.tcp.send_with_wakeups(
            client,
            format!("Host: {SOAK_ADDRESS}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        )?;
        self.counters.handler_wakeups += wakeups.len();
        if self.processes.get(process).is_none() {
            return Err("VM HTTP slow-client handler disappeared".to_string());
        }
        self.processes
            .with_process_control_mutator(process, |actor| actor.wake())?;
        let complete_tick = self.next_tick();
        let complete = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            complete_tick,
            |request| soak_response(request, &mut self.stateful_counter),
        )?;
        self.record_poll(&complete);
        self.record_response_memory();
        drain_client(&mut self.tcp, client)?;
        self.tcp.close_stream(client)?;
        self.counters.terminals.push(VmHttpSoakTerminal {
            phase: "slow-client-write",
            outcome: "completed",
            diagnostic: "request resumed through VM TCP readiness".to_string(),
        });
        Ok(())
    }

    fn cancel_parked_handler(&mut self) -> Result<(), String> {
        let client = self.tcp.connect(SOAK_ADDRESS, "cancel-client")?;
        let tick = self.next_tick();
        let accepted = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            tick,
            |_request| Err("cancelled request reached handler".to_string()),
        )?;
        self.record_poll(&accepted);
        let process = self.active_handler("cancellation-burst")?;
        self.server
            .cancel_handler(
                &mut self.processes,
                &mut self.tcp,
                process,
                VmExitReason::Killed,
            )?
            .ok_or_else(|| "VM HTTP cancellation handler was not active".to_string())?;
        let cleanup_tick = self.next_tick();
        let cleanup = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            cleanup_tick,
            |_request| Err("cancelled request reached cleanup handler".to_string()),
        )?;
        self.record_poll(&cleanup);
        self.tcp.close_stream(client)?;
        self.counters.expected_failures += 1;
        self.counters.terminals.push(VmHttpSoakTerminal {
            phase: "cancellation-burst",
            outcome: "cancelled",
            diagnostic: format!("handler {} exited with killed", process.as_u64()),
        });
        Ok(())
    }

    fn expire_parked_handler(&mut self) -> Result<(), String> {
        let client = self.tcp.connect(SOAK_ADDRESS, "deadline-client")?;
        let accepted_tick = self.next_tick();
        let accepted = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            accepted_tick,
            |_request| Err("deadline request reached handler".to_string()),
        )?;
        self.record_poll(&accepted);
        let deadline_tick = accepted_tick + HANDLER_TIMEOUT_TICKS;
        self.tick = deadline_tick;
        let expired = self.server.poll_keep_alive_with_deadlines(
            &mut self.processes,
            &mut self.tcp,
            &mut self.timers,
            &mut self.scheduler,
            deadline_tick,
            |_request| Err("expired request reached handler".to_string()),
        )?;
        self.record_poll(&expired);
        if expired.timed_out_handlers.len() != 1 {
            return Err("VM HTTP soak deadline did not expire exactly one handler".to_string());
        }
        self.tcp.close_stream(client)?;
        self.counters.expected_failures += 1;
        self.counters.terminals.push(VmHttpSoakTerminal {
            phase: "request-deadline",
            outcome: "timed_out",
            diagnostic: "http_request_deadline_exceeded".to_string(),
        });
        Ok(())
    }

    fn reject_request(
        &mut self,
        phase: &'static str,
        request: Vec<u8>,
        close_write: bool,
        expected: &str,
    ) -> Result<(), String> {
        let client = self.tcp.connect(SOAK_ADDRESS, format!("{phase}-client"))?;
        self.tcp.send(client, request)?;
        if close_write {
            self.tcp.close_write(client)?;
        }
        let tick = self.next_tick();
        let error = self
            .server
            .poll_keep_alive_with_deadlines(
                &mut self.processes,
                &mut self.tcp,
                &mut self.timers,
                &mut self.scheduler,
                tick,
                |_request| Err(format!("{phase} reached handler")),
            )
            .expect_err("adversarial HTTP request must be rejected");
        if error != expected {
            return Err(format!(
                "VM HTTP soak `{phase}` diagnostic mismatch: expected `{expected}`, observed `{error}`"
            ));
        }
        let process = self.active_handler(phase)?;
        self.server
            .cancel_handler(
                &mut self.processes,
                &mut self.tcp,
                process,
                VmExitReason::Error(error.clone()),
            )?
            .ok_or_else(|| format!("VM HTTP soak `{phase}` handler was not active"))?;
        self.tcp.close_stream(client)?;
        self.counters.expected_failures += 1;
        self.counters.terminals.push(VmHttpSoakTerminal {
            phase,
            outcome: "rejected",
            diagnostic: error,
        });
        Ok(())
    }

    fn active_handler(
        &self,
        phase: &str,
    ) -> Result<crate::runtime::vm::process::VmProcessId, String> {
        self.server
            .handlers
            .last()
            .map(|handler| handler.process)
            .ok_or_else(|| format!("VM HTTP soak `{phase}` has no active handler"))
    }

    fn record_poll(&mut self, poll: &super::deadline::VmHttpDeadlinePoll) {
        self.counters.parked_handlers += poll.http.parked;
        for event in &poll.timer_events {
            match event {
                VmTimerEvent::Cancelled { .. } => self.counters.timer_cancellations += 1,
                VmTimerEvent::Fired { .. } | VmTimerEvent::DeadlineMissed { .. } => {
                    self.counters.timer_expirations += 1;
                }
                VmTimerEvent::Coalesced { .. }
                | VmTimerEvent::Overflow { .. }
                | VmTimerEvent::OwnerExited { .. } => {}
            }
        }
    }

    fn record_response_memory(&mut self) {
        for process in &self.server.last_completed_handlers {
            if let Some(metrics) = self.server.response_memory_metrics(*process) {
                self.counters.response_memory_high_water_bytes = self
                    .counters
                    .response_memory_high_water_bytes
                    .max(metrics.high_water_bytes);
                self.counters.response_memory_retained_bytes = self
                    .counters
                    .response_memory_retained_bytes
                    .saturating_add(metrics.current_bytes);
            }
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }

    fn resource_snapshot(&self) -> VmHttpSoakResourceSnapshot {
        resource_snapshot(
            self.processes.metrics(),
            self.tcp.metrics(),
            &self.server,
            self.timers.snapshots().len(),
        )
    }

    fn capture_phase(
        &mut self,
        phase: &'static str,
        replay: usize,
        operation: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        let before = self.resource_snapshot();
        let result = operation(self);
        self.record_phase(phase, Some(replay), before);
        result
    }

    fn record_phase(
        &mut self,
        phase: &'static str,
        replay: Option<usize>,
        before: VmHttpSoakResourceSnapshot,
    ) {
        let after = self.resource_snapshot();
        self.counters
            .phase_resources
            .push(phase_resources(phase, replay, before, after));
    }
}

fn soak_response(
    request: ::http::Request<String>,
    stateful_counter: &mut usize,
) -> Result<::http::Response<String>, String> {
    let (status, body) = match request.uri().path() {
        "/static" => (200, "hello".to_string()),
        "/json" => (200, "{\"ok\":true}".to_string()),
        "/add" => (200, "3".to_string()),
        "/users/42" => (200, "user:42".to_string()),
        "/counter" => {
            *stateful_counter += 1;
            (200, stateful_counter.to_string())
        }
        _ => (404, "not found".to_string()),
    };
    ::http::Response::builder()
        .status(status)
        .body(body)
        .map_err(|error| error.to_string())
}

fn drain_client(tcp: &mut VmTcpRuntime, client: VmTcpStream) -> Result<(), String> {
    let mut response_bytes = 0usize;
    while let Some(bytes) = tcp.receive(client, 4096)? {
        response_bytes = response_bytes.saturating_add(bytes.len());
    }
    if response_bytes == 0 {
        return Err("VM HTTP soak completed request without response bytes".to_string());
    }
    Ok(())
}

fn resource_snapshot(
    processes: VmProcessTableMetrics,
    tcp: VmTcpRuntimeMetrics,
    server: &VmHttpTcpServer,
    active_timers: usize,
) -> VmHttpSoakResourceSnapshot {
    let requests = server.request_resource_metrics();
    VmHttpSoakResourceSnapshot {
        process_total: processes.total_processes,
        process_live: processes.live_processes,
        process_exited: processes.exited_processes,
        mailbox_messages: processes.mailbox_messages,
        heap_bytes: processes.heap_bytes,
        resource_handles: processes.resource_handles,
        native_boundary_handles: processes.resource_handles,
        listener_total: tcp.listeners,
        listener_open: tcp.open_listeners,
        stream_total: tcp.streams,
        stream_open: tcp.open_streams,
        queued_accepts: tcp.queued_accepts,
        queued_messages: tcp.queued_messages,
        queued_bytes: tcp.queued_bytes,
        waiting_readers: tcp.waiting_readers,
        waiting_writers: tcp.waiting_writers,
        active_handlers: server.active_handlers(),
        active_timers,
        active_body_buffers: requests.active_body_buffers,
        active_body_bytes: requests.active_body_bytes,
        active_telemetry_spans: requests.active_telemetry_spans,
        active_route_contexts: requests.active_route_contexts,
    }
}

fn rate_bps(errors: usize, requests: usize) -> usize {
    if requests == 0 {
        return 0;
    }
    errors.saturating_mul(10_000) / requests
}

fn ratio_milli(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_mul(1_000) / denominator
}

fn write_report(report: &VmHttpSoakReport) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("failed to serialize VM HTTP soak report: {error}"))?;
    if let Some(parent) = report.report_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create VM HTTP soak report directory: {error}"))?;
    }
    std::fs::write(&report.report_path, format!("{json}\n"))
        .map_err(|error| format!("failed to write VM HTTP soak report: {error}"))
}
