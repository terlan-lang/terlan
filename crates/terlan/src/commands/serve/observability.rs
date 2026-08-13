//! VM-owned, bounded observability for serve lifecycle and protocol work.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::config::EffectiveServeConfig;
use super::manifest;

pub(super) const OBSERVABILITY_SCHEMA: &str = "terlan-vm-observability-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum VmEventDomain {
    Process,
    Socket,
    Request,
    Scheduler,
    Capability,
    Tls,
    Cleanup,
}

const ALL_EVENT_DOMAINS: [VmEventDomain; 7] = [
    VmEventDomain::Process,
    VmEventDomain::Socket,
    VmEventDomain::Request,
    VmEventDomain::Scheduler,
    VmEventDomain::Capability,
    VmEventDomain::Tls,
    VmEventDomain::Cleanup,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum VmEventStatus {
    Started,
    Ready,
    Completed,
    Rejected,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VmServeEvent {
    schema: &'static str,
    sequence: u64,
    config_fingerprint: String,
    domain: VmEventDomain,
    name: String,
    status: VmEventStatus,
    request_id: Option<u64>,
    connection_id: Option<u64>,
    actor_id: Option<u64>,
    route_id: Option<String>,
    trace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VmServeTrace {
    schema: &'static str,
    config_fingerprint: String,
    trace_id: String,
    parent_span_id: String,
    sampled: bool,
    event_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VmServeMetrics {
    schema: &'static str,
    config_fingerprint: String,
    capacity: usize,
    recorded_events: u64,
    dropped_events: u64,
    repeated_signals: u64,
    forced_shutdowns: u64,
    domain_events: BTreeMap<VmEventDomain, u64>,
    status_events: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmTraceContext {
    trace_id: String,
    parent_span_id: String,
    sampled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct VmEventCorrelation<'a> {
    pub(super) request_id: Option<u64>,
    pub(super) connection_id: Option<u64>,
    pub(super) actor_id: Option<u64>,
    pub(super) route_id: Option<&'a str>,
    pub(super) trace: Option<&'a VmTraceContext>,
}

#[derive(Debug)]
pub(super) struct VmServeObservability {
    fingerprint: String,
    capacity: usize,
    events: Vec<VmServeEvent>,
    traces: Vec<VmServeTrace>,
    metrics: VmServeMetrics,
    next_sequence: u64,
    termination_seen: bool,
    draining: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmObservabilityArtifacts {
    pub(super) events: PathBuf,
    pub(super) metrics: PathBuf,
    pub(super) traces: PathBuf,
}

impl VmServeObservability {
    pub(super) fn new(config: &EffectiveServeConfig, capacity: usize) -> super::ServeResult<Self> {
        if capacity == 0 {
            return Err(
                "error[vm.observability.capacity]: capacity must be greater than zero".into(),
            );
        }
        Ok(Self {
            fingerprint: config.fingerprint.clone(),
            capacity,
            events: Vec::with_capacity(capacity.min(4_096)),
            traces: Vec::with_capacity(capacity.min(4_096)),
            metrics: VmServeMetrics {
                schema: OBSERVABILITY_SCHEMA,
                config_fingerprint: config.fingerprint.clone(),
                capacity,
                recorded_events: 0,
                dropped_events: 0,
                repeated_signals: 0,
                forced_shutdowns: 0,
                domain_events: ALL_EVENT_DOMAINS
                    .into_iter()
                    .map(|domain| (domain, 0))
                    .collect(),
                status_events: BTreeMap::new(),
            },
            next_sequence: 1,
            termination_seen: false,
            draining: false,
        })
    }

    pub(super) fn record(
        &mut self,
        domain: VmEventDomain,
        name: impl Into<String>,
        status: VmEventStatus,
    ) {
        self.record_correlated(domain, name, status, VmEventCorrelation::default());
    }

    pub(super) fn record_correlated(
        &mut self,
        domain: VmEventDomain,
        name: impl Into<String>,
        status: VmEventStatus,
        correlation: VmEventCorrelation<'_>,
    ) {
        let name = name.into();
        if name.trim().is_empty() || name.len() > 96 {
            self.metrics.dropped_events = self.metrics.dropped_events.saturating_add(1);
            return;
        }
        if self.events.len() == self.capacity {
            self.metrics.dropped_events = self.metrics.dropped_events.saturating_add(1);
            return;
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.metrics.recorded_events = self.metrics.recorded_events.saturating_add(1);
        let count = self.metrics.domain_events.entry(domain).or_insert(0);
        *count = count.saturating_add(1);
        let count = self
            .metrics
            .status_events
            .entry(status_name(status).to_string())
            .or_insert(0);
        *count = count.saturating_add(1);
        if let Some(trace) = correlation.trace {
            self.traces.push(VmServeTrace {
                schema: OBSERVABILITY_SCHEMA,
                config_fingerprint: self.fingerprint.clone(),
                trace_id: trace.trace_id.clone(),
                parent_span_id: trace.parent_span_id.clone(),
                sampled: trace.sampled,
                event_sequence: sequence,
            });
        }
        self.events.push(VmServeEvent {
            schema: OBSERVABILITY_SCHEMA,
            sequence,
            config_fingerprint: self.fingerprint.clone(),
            domain,
            name,
            status,
            request_id: correlation.request_id,
            connection_id: correlation.connection_id,
            actor_id: correlation.actor_id,
            route_id: correlation.route_id.map(str::to_string),
            trace_id: correlation.trace.map(|context| context.trace_id.clone()),
        });
    }

    pub(super) fn begin_shutdown(&mut self, trigger: &str) {
        if self.termination_seen {
            self.metrics.repeated_signals = self.metrics.repeated_signals.saturating_add(1);
            self.record(
                VmEventDomain::Process,
                "shutdown.signal.repeated",
                VmEventStatus::Rejected,
            );
            return;
        }
        self.termination_seen = true;
        self.draining = true;
        self.record(
            VmEventDomain::Process,
            format!("shutdown.trigger.{}", trigger.to_ascii_lowercase()),
            VmEventStatus::Started,
        );
        self.record(
            VmEventDomain::Scheduler,
            "shutdown.drain",
            VmEventStatus::Started,
        );
    }

    pub(super) fn finish_shutdown(&mut self, timed_out: bool) {
        if timed_out {
            self.metrics.forced_shutdowns = self.metrics.forced_shutdowns.saturating_add(1);
            self.record(
                VmEventDomain::Scheduler,
                "shutdown.deadline",
                VmEventStatus::TimedOut,
            );
        } else if self.termination_seen && self.draining {
            self.record(
                VmEventDomain::Scheduler,
                "shutdown.drain",
                VmEventStatus::Completed,
            );
        }
        self.record(
            VmEventDomain::Cleanup,
            "resources.cleanup",
            VmEventStatus::Completed,
        );
        self.record(
            VmEventDomain::Process,
            "process.exit",
            VmEventStatus::Completed,
        );
    }

    pub(super) fn flush(&self, web_root: &Path) -> super::ServeResult<VmObservabilityArtifacts> {
        let root =
            manifest::adjacent_project_root(web_root).unwrap_or_else(|| web_root.to_path_buf());
        let directory = root.join("build/artifacts");
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "error[vm.observability.flush]: create {}: {error}",
                directory.display()
            )
        })?;
        let artifacts = VmObservabilityArtifacts {
            events: directory.join("vm-runtime-observability-events.jsonl"),
            metrics: directory.join("vm-runtime-observability-metrics.json"),
            traces: directory.join("vm-runtime-observability-traces.json"),
        };
        let mut jsonl = Vec::new();
        for event in &self.events {
            serde_json::to_writer(&mut jsonl, event)
                .map_err(|error| format!("error[vm.observability.flush]: encode event: {error}"))?;
            jsonl.push(b'\n');
        }
        atomic_write(&artifacts.events, &jsonl)?;
        let metrics = serde_json::to_vec_pretty(&self.metrics)
            .map_err(|error| format!("error[vm.observability.flush]: encode metrics: {error}"))?;
        atomic_write(&artifacts.metrics, &metrics)?;
        let traces = serde_json::to_vec_pretty(&self.traces)
            .map_err(|error| format!("error[vm.observability.flush]: encode traces: {error}"))?;
        atomic_write(&artifacts.traces, &traces)?;
        Ok(artifacts)
    }

    #[cfg(test)]
    fn metrics(&self) -> &VmServeMetrics {
        &self.metrics
    }
}

pub(super) fn parse_traceparent(value: &str) -> super::ServeResult<VmTraceContext> {
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 4
        || parts[0] != "00"
        || parts[1].len() != 32
        || parts[2].len() != 16
        || parts[3].len() != 2
        || !parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || parts[1].bytes().all(|byte| byte == b'0')
        || parts[2].bytes().all(|byte| byte == b'0')
    {
        return Err("error[vm.observability.traceparent]: malformed W3C traceparent".into());
    }
    let flags = u8::from_str_radix(parts[3], 16)
        .map_err(|_| "error[vm.observability.traceparent]: invalid trace flags".to_string())?;
    Ok(VmTraceContext {
        trace_id: parts[1].to_ascii_lowercase(),
        parent_span_id: parts[2].to_ascii_lowercase(),
        sampled: flags & 1 == 1,
    })
}

fn status_name(status: VmEventStatus) -> &'static str {
    match status {
        VmEventStatus::Started => "started",
        VmEventStatus::Ready => "ready",
        VmEventStatus::Completed => "completed",
        VmEventStatus::Rejected => "rejected",
        VmEventStatus::Failed => "failed",
        VmEventStatus::TimedOut => "timed-out",
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> super::ServeResult<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(|error| {
        format!(
            "error[vm.observability.flush]: write {}: {error}",
            temporary.display()
        )
    })?;
    Ok(fs::rename(&temporary, path).map_err(|error| {
        format!(
            "error[vm.observability.flush]: publish {}: {error}",
            path.display()
        )
    })?)
}

#[cfg(test)]
#[path = "observability_test.rs"]
mod observability_test;
