#![allow(dead_code)]

use std::collections::HashMap;

use super::process::{VmProcessId, VmProcessTable};
use super::support_bundle::{
    VmSupportBundleReplayRecorder, VmSupportBundleReplayResource,
    VmSupportBundleReplayResourceKind, VmSupportBundleReplayStep,
};
use super::timer::{VmTimerEvent, VmTimerId, VmTimerTable};

#[cfg(test)]
#[path = "acme_worker_test.rs"]
mod acme_worker_test;

/// VM-owned handle for one ACME worker lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmAcmeWorkerHandle(u64);

impl VmAcmeWorkerHandle {
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// ACME endpoint mode captured by the VM for cache provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmAcmeMode {
    Staging,
    Live,
}

/// VM-owned retry and jitter policy for ACME renewal attempts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeRenewalRetryPolicy {
    pub(crate) max_attempts: u8,
    pub(crate) base_delay_ticks: u64,
    pub(crate) jitter_seed: u64,
}

impl VmAcmeRenewalRetryPolicy {
    pub(crate) fn new(
        max_attempts: u8,
        base_delay_ticks: u64,
        jitter_seed: u64,
    ) -> Result<Self, String> {
        if max_attempts == 0 {
            return Err("ACME renewal retry policy must allow at least one attempt".to_string());
        }
        if base_delay_ticks == 0 {
            return Err("ACME renewal retry base delay must be positive".to_string());
        }
        Ok(Self {
            max_attempts,
            base_delay_ticks,
            jitter_seed,
        })
    }

    pub(crate) fn delay_for_attempt(&self, attempt: u8) -> Result<u64, String> {
        if attempt == 0 {
            return Err("ACME renewal retry attempt must be one-based".to_string());
        }
        if attempt > self.max_attempts {
            return Err(format!(
                "ACME renewal retry attempt {attempt} exceeds max {}",
                self.max_attempts
            ));
        }
        let exponent = u32::from(attempt.saturating_sub(1)).min(31);
        let exponential_delay = self.base_delay_ticks.saturating_mul(1_u64 << exponent);
        Ok(exponential_delay.saturating_add(self.jitter_for_attempt(attempt)))
    }

    fn jitter_for_attempt(&self, attempt: u8) -> u64 {
        let mixed = self
            .jitter_seed
            .wrapping_add(u64::from(attempt).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .rotate_left(u32::from(attempt % 31) + 1);
        mixed % self.base_delay_ticks
    }
}

/// Typed ACME worker request owned by the VM scheduler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeWorkerRequest {
    pub(crate) domain: String,
    pub(crate) account_id: String,
    pub(crate) cache_key: String,
    pub(crate) mode: VmAcmeMode,
}

impl VmAcmeWorkerRequest {
    pub(crate) fn new(
        domain: impl Into<String>,
        account_id: impl Into<String>,
        cache_key: impl Into<String>,
        mode: VmAcmeMode,
    ) -> Self {
        Self {
            domain: domain.into(),
            account_id: account_id.into(),
            cache_key: cache_key.into(),
            mode,
        }
    }
}

/// VM-visible execution lane for the same ACME worker contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmAcmeWorkerExecutionLane {
    DeterministicFixture { fixture_id: String },
    Live { directory_url: String },
}

/// HTTP-01 challenge routing data produced by maintained ACME machinery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeHttp01Challenge {
    pub(crate) token: String,
    pub(crate) key_authorization: String,
    pub(crate) route: String,
}

/// VM-visible ACME worker state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmAcmeWorkerState {
    Requested,
    ChallengeReady(VmAcmeHttp01Challenge),
    Issuing,
    CacheWriting { cache_version: u64 },
    RenewalScheduled { not_before_epoch_secs: u64 },
    Completed,
    Cancelled { reason: String },
    Shutdown,
}

/// Scheduler wake emitted when ACME worker state becomes runnable/observable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmAcmeWorkerWake {
    RenewalDue {
        owner: VmProcessId,
        worker: VmAcmeWorkerHandle,
        not_before_epoch_secs: u64,
    },
    IssuanceReady {
        process: VmProcessId,
        worker: VmAcmeWorkerHandle,
    },
    ChallengeReady {
        owner: VmProcessId,
        worker: VmAcmeWorkerHandle,
    },
    CacheWriteReady {
        owner: VmProcessId,
        worker: VmAcmeWorkerHandle,
    },
    Terminal {
        owner: VmProcessId,
        worker: VmAcmeWorkerHandle,
    },
}

/// Runtime-inspectable ACME worker summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeWorkerInfo {
    pub(crate) owner: VmProcessId,
    pub(crate) request: VmAcmeWorkerRequest,
    pub(crate) execution_lane: VmAcmeWorkerExecutionLane,
    pub(crate) state: VmAcmeWorkerState,
    pub(crate) event_count: usize,
    pub(crate) closed: bool,
}

/// Typed telemetry span emitted by the VM ACME worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeWorkerTelemetrySpan {
    pub(crate) worker: VmAcmeWorkerHandle,
    pub(crate) owner: VmProcessId,
    pub(crate) name: String,
    pub(crate) domain: String,
    pub(crate) mode: VmAcmeMode,
    pub(crate) terminal: bool,
}

/// VM-owned access-policy decision for ACME HTTP-01 routing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmAcmeWorkerAccessDecision {
    Allow { route: String },
    Deny { reason: String },
}

/// VM-owned ACME renewal actor state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmAcmeRenewalActorState {
    Waiting,
    Renewing,
    Shutdown,
}

/// VM-owned ACME renewal actor.
///
/// Inputs: one completed ACME worker, its owner process, and a VM timer.
/// Output: inspectable actor lifecycle for renewal due/shutdown handling.
/// Transformation: binds renewal work to VM process/timer ownership rather
/// than host-runtime timers or external task handles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeRenewalActor {
    pub(crate) owner: VmProcessId,
    pub(crate) worker: VmAcmeWorkerHandle,
    pub(crate) renewal_timer: Option<VmTimerId>,
    pub(crate) state: VmAcmeRenewalActorState,
}

/// VM-owned shutdown result for an ACME renewal actor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcmeRenewalActorShutdown {
    pub(crate) cancelled_timer: Option<VmTimerEvent>,
    pub(crate) terminal_wake: VmAcmeWorkerWake,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmAcmeWorker {
    owner: VmProcessId,
    request: VmAcmeWorkerRequest,
    execution_lane: VmAcmeWorkerExecutionLane,
    state: VmAcmeWorkerState,
    events: Vec<String>,
    telemetry_spans: Vec<VmAcmeWorkerTelemetrySpan>,
    issuance_waiters: Vec<VmProcessId>,
    closed: bool,
}

/// VM-owned ACME worker registry.
#[derive(Debug, Default)]
pub(crate) struct VmAcmeWorkerRuntime {
    next_worker: u64,
    max_open_workers_per_owner: Option<usize>,
    workers: HashMap<VmAcmeWorkerHandle, VmAcmeWorker>,
}

impl VmAcmeWorkerRuntime {
    pub(crate) fn new() -> Self {
        Self {
            next_worker: 1,
            max_open_workers_per_owner: None,
            workers: HashMap::new(),
        }
    }

    pub(crate) fn with_owner_limit(max_open_workers_per_owner: usize) -> Result<Self, String> {
        if max_open_workers_per_owner == 0 {
            return Err("ACME worker owner limit must be positive".to_string());
        }
        Ok(Self {
            next_worker: 1,
            max_open_workers_per_owner: Some(max_open_workers_per_owner),
            workers: HashMap::new(),
        })
    }

    pub(crate) fn start_worker(
        &mut self,
        owner: VmProcessId,
        request: VmAcmeWorkerRequest,
    ) -> Result<VmAcmeWorkerHandle, String> {
        self.start_worker_for_lane(
            owner,
            request,
            VmAcmeWorkerExecutionLane::DeterministicFixture {
                fixture_id: "default".to_string(),
            },
        )
    }

    pub(crate) fn start_worker_for_lane(
        &mut self,
        owner: VmProcessId,
        request: VmAcmeWorkerRequest,
        execution_lane: VmAcmeWorkerExecutionLane,
    ) -> Result<VmAcmeWorkerHandle, String> {
        validate_request(&request)?;
        validate_execution_lane(&execution_lane)?;
        self.enforce_owner_backpressure(owner)?;
        let handle = VmAcmeWorkerHandle(self.next_worker);
        self.next_worker = self.next_worker.saturating_add(1);
        self.workers.insert(
            handle,
            VmAcmeWorker {
                owner,
                request,
                execution_lane,
                state: VmAcmeWorkerState::Requested,
                events: vec!["request accepted".to_string()],
                telemetry_spans: Vec::new(),
                issuance_waiters: Vec::new(),
                closed: false,
            },
        );
        self.record_telemetry_span(handle, "acme.worker.request", false)?;
        Ok(handle)
    }

    pub(crate) fn prepare_http01_challenge(
        &mut self,
        handle: VmAcmeWorkerHandle,
        token: impl Into<String>,
        key_authorization: impl Into<String>,
    ) -> Result<VmAcmeWorkerWake, String> {
        let token = token.into();
        let key_authorization = key_authorization.into();
        validate_http01_token(&token)?;
        if key_authorization.trim().is_empty() {
            return Err("ACME HTTP-01 key authorization must not be empty".to_string());
        }

        let worker = self.worker_mut(handle)?;
        ensure_state(&worker.state, &[VmAcmeWorkerStateName::Requested])?;
        let challenge = VmAcmeHttp01Challenge {
            route: format!("/.well-known/acme-challenge/{token}"),
            token,
            key_authorization,
        };
        worker.state = VmAcmeWorkerState::ChallengeReady(challenge);
        worker.events.push("challenge prepared".to_string());
        self.record_telemetry_span(handle, "acme.challenge.ready", false)?;
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerWake::ChallengeReady {
            owner: worker.owner,
            worker: handle,
        })
    }

    pub(crate) fn start_issuance(
        &mut self,
        handle: VmAcmeWorkerHandle,
    ) -> Result<Vec<VmAcmeWorkerWake>, String> {
        let worker = self.worker_mut(handle)?;
        ensure_state(
            &worker.state,
            &[
                VmAcmeWorkerStateName::Requested,
                VmAcmeWorkerStateName::ChallengeReady,
            ],
        )?;
        worker.state = VmAcmeWorkerState::Issuing;
        worker.events.push("issuance started".to_string());
        let wakes = worker
            .issuance_waiters
            .drain(..)
            .map(|process| VmAcmeWorkerWake::IssuanceReady {
                process,
                worker: handle,
            })
            .collect();
        self.record_telemetry_span(handle, "acme.issuance.started", false)?;
        Ok(wakes)
    }

    pub(crate) fn begin_cache_write(
        &mut self,
        handle: VmAcmeWorkerHandle,
        cache_version: u64,
    ) -> Result<VmAcmeWorkerWake, String> {
        if cache_version == 0 {
            return Err("ACME cache version must be positive".to_string());
        }
        let worker = self.worker_mut(handle)?;
        ensure_state(&worker.state, &[VmAcmeWorkerStateName::Issuing])?;
        worker.state = VmAcmeWorkerState::CacheWriting { cache_version };
        worker.events.push("cache write attempted".to_string());
        self.record_telemetry_span(handle, "acme.cache.write", false)?;
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerWake::CacheWriteReady {
            owner: worker.owner,
            worker: handle,
        })
    }

    pub(crate) fn complete_worker(
        &mut self,
        handle: VmAcmeWorkerHandle,
    ) -> Result<VmAcmeWorkerWake, String> {
        let worker = self.worker_mut(handle)?;
        ensure_state(&worker.state, &[VmAcmeWorkerStateName::CacheWriting])?;
        worker.state = VmAcmeWorkerState::Completed;
        worker.closed = true;
        worker.events.push("worker completed".to_string());
        self.record_telemetry_span(handle, "acme.worker.completed", true)?;
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerWake::Terminal {
            owner: worker.owner,
            worker: handle,
        })
    }

    pub(crate) fn schedule_renewal(
        &mut self,
        handle: VmAcmeWorkerHandle,
        not_before_epoch_secs: u64,
    ) -> Result<(), String> {
        if not_before_epoch_secs == 0 {
            return Err("ACME renewal timestamp must be positive".to_string());
        }
        let worker = self.worker_mut(handle)?;
        ensure_state(&worker.state, &[VmAcmeWorkerStateName::Completed])?;
        worker.state = VmAcmeWorkerState::RenewalScheduled {
            not_before_epoch_secs,
        };
        worker.events.push("renewal decision recorded".to_string());
        self.record_telemetry_span(handle, "acme.renewal.scheduled", false)?;
        Ok(())
    }

    pub(crate) fn schedule_renewal_timer(
        &mut self,
        handle: VmAcmeWorkerHandle,
        processes: &VmProcessTable,
        timers: &mut VmTimerTable,
        not_before_epoch_secs: u64,
    ) -> Result<VmTimerId, String> {
        if not_before_epoch_secs == 0 {
            return Err("ACME renewal timestamp must be positive".to_string());
        }
        let owner = {
            let worker = self.worker(handle)?;
            ensure_state(&worker.state, &[VmAcmeWorkerStateName::Completed])?;
            worker.owner
        };
        let timer = timers.start_one_shot(processes, owner, not_before_epoch_secs)?;
        self.schedule_renewal(handle, not_before_epoch_secs)?;
        Ok(timer)
    }

    /// Spawns a VM-owned ACME renewal actor for a completed worker.
    pub(crate) fn spawn_renewal_actor(
        &mut self,
        processes: &mut VmProcessTable,
        timers: &mut VmTimerTable,
        handle: VmAcmeWorkerHandle,
        not_before_epoch_secs: u64,
    ) -> Result<VmAcmeRenewalActor, String> {
        let owner = self.worker(handle)?.owner;
        let timer =
            self.schedule_renewal_timer(handle, processes, timers, not_before_epoch_secs)?;
        let resource_handle = renewal_actor_resource_handle(handle);
        processes
            .get_mut(owner)
            .ok_or_else(|| format!("missing ACME renewal actor owner {}", owner.as_u64()))?
            .add_resource_handle(resource_handle);
        Ok(VmAcmeRenewalActor {
            owner,
            worker: handle,
            renewal_timer: Some(timer),
            state: VmAcmeRenewalActorState::Waiting,
        })
    }

    pub(crate) fn renewal_due_wakeups(&self, now_epoch_secs: u64) -> Vec<VmAcmeWorkerWake> {
        self.workers
            .iter()
            .filter_map(|(handle, worker)| {
                let VmAcmeWorkerState::RenewalScheduled {
                    not_before_epoch_secs,
                } = worker.state
                else {
                    return None;
                };
                if not_before_epoch_secs <= now_epoch_secs {
                    Some(VmAcmeWorkerWake::RenewalDue {
                        owner: worker.owner,
                        worker: *handle,
                        not_before_epoch_secs,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn begin_due_renewal(
        &mut self,
        handle: VmAcmeWorkerHandle,
        now_epoch_secs: u64,
    ) -> Result<VmAcmeWorkerWake, String> {
        let worker = self.worker_mut(handle)?;
        let VmAcmeWorkerState::RenewalScheduled {
            not_before_epoch_secs,
        } = worker.state
        else {
            return Err(format!(
                "ACME worker {} has no scheduled renewal",
                handle.as_u64()
            ));
        };
        if now_epoch_secs < not_before_epoch_secs {
            return Err(format!(
                "ACME renewal is not due until {not_before_epoch_secs}; now={now_epoch_secs}"
            ));
        }
        worker.state = VmAcmeWorkerState::Requested;
        worker.closed = false;
        worker.events.push("renewal attempt started".to_string());
        self.record_telemetry_span(handle, "acme.renewal.started", false)?;
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerWake::RenewalDue {
            owner: worker.owner,
            worker: handle,
            not_before_epoch_secs,
        })
    }

    pub(crate) fn cancel_worker(
        &mut self,
        handle: VmAcmeWorkerHandle,
        reason: impl Into<String>,
    ) -> Result<VmAcmeWorkerWake, String> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err("ACME cancellation reason must not be empty".to_string());
        }
        let worker = self.worker_mut(handle)?;
        if worker.closed {
            return Err(format!("ACME worker {} is closed", handle.as_u64()));
        }
        worker.state = VmAcmeWorkerState::Cancelled { reason };
        worker.closed = true;
        worker
            .events
            .push("worker cancellation observed".to_string());
        self.record_telemetry_span(handle, "acme.worker.cancelled", true)?;
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerWake::Terminal {
            owner: worker.owner,
            worker: handle,
        })
    }

    pub(crate) fn shutdown_worker(
        &mut self,
        handle: VmAcmeWorkerHandle,
    ) -> Result<VmAcmeWorkerWake, String> {
        let worker = self.worker_mut(handle)?;
        worker.state = VmAcmeWorkerState::Shutdown;
        worker.closed = true;
        worker.events.push("worker shutdown observed".to_string());
        self.record_telemetry_span(handle, "acme.worker.shutdown", true)?;
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerWake::Terminal {
            owner: worker.owner,
            worker: handle,
        })
    }

    pub(crate) fn shutdown_owner_workers(&mut self, owner: VmProcessId) -> Vec<VmAcmeWorkerWake> {
        let mut wakes = Vec::new();
        for (handle, worker) in &mut self.workers {
            if worker.owner == owner && !worker.closed {
                worker.state = VmAcmeWorkerState::Shutdown;
                worker.closed = true;
                worker.events.push("worker shutdown observed".to_string());
                worker.telemetry_spans.push(VmAcmeWorkerTelemetrySpan {
                    worker: *handle,
                    owner,
                    name: "acme.worker.shutdown".to_string(),
                    domain: worker.request.domain.clone(),
                    mode: worker.request.mode,
                    terminal: true,
                });
                wakes.push(VmAcmeWorkerWake::Terminal {
                    owner,
                    worker: *handle,
                });
            }
        }
        wakes
    }

    pub(crate) fn inspect_worker(
        &self,
        handle: VmAcmeWorkerHandle,
    ) -> Result<VmAcmeWorkerInfo, String> {
        let worker = self.worker(handle)?;
        Ok(VmAcmeWorkerInfo {
            owner: worker.owner,
            request: worker.request.clone(),
            execution_lane: worker.execution_lane.clone(),
            state: worker.state.clone(),
            event_count: worker.events.len(),
            closed: worker.closed,
        })
    }

    pub(crate) fn park_issuance_waiter(
        &mut self,
        handle: VmAcmeWorkerHandle,
        process: VmProcessId,
    ) -> Result<(), String> {
        let worker = self.worker_mut(handle)?;
        if worker.closed {
            return Err(format!("ACME worker {} is closed", handle.as_u64()));
        }
        if matches!(worker.state, VmAcmeWorkerState::Issuing) {
            return Err("ACME issuance has already started".to_string());
        }
        if !worker.issuance_waiters.contains(&process) {
            worker.issuance_waiters.push(process);
        }
        Ok(())
    }

    pub(crate) fn telemetry_spans(
        &self,
        handle: VmAcmeWorkerHandle,
    ) -> Result<Vec<VmAcmeWorkerTelemetrySpan>, String> {
        Ok(self.worker(handle)?.telemetry_spans.clone())
    }

    pub(crate) fn challenge_route_access_decision(
        &self,
        handle: VmAcmeWorkerHandle,
        method: &str,
        route: &str,
    ) -> Result<VmAcmeWorkerAccessDecision, String> {
        let worker = self.worker(handle)?;
        let VmAcmeWorkerState::ChallengeReady(challenge) = &worker.state else {
            return Ok(VmAcmeWorkerAccessDecision::Deny {
                reason: "ACME HTTP-01 challenge is not ready".to_string(),
            });
        };
        if !matches!(method, "GET" | "HEAD") {
            return Ok(VmAcmeWorkerAccessDecision::Deny {
                reason: format!("ACME HTTP-01 method `{method}` is not allowed"),
            });
        }
        if route != challenge.route {
            return Ok(VmAcmeWorkerAccessDecision::Deny {
                reason: "ACME HTTP-01 route does not match worker challenge".to_string(),
            });
        }
        Ok(VmAcmeWorkerAccessDecision::Allow {
            route: challenge.route.clone(),
        })
    }

    pub(crate) fn capture_support_bundle_step(
        &self,
        handle: VmAcmeWorkerHandle,
        recorder: &mut VmSupportBundleReplayRecorder,
    ) -> Result<VmSupportBundleReplayStep, String> {
        let worker = self.worker(handle)?;
        let resource = VmSupportBundleReplayResource::new(
            VmSupportBundleReplayResourceKind::AcmeWorker,
            format!("acme-worker:{}", handle.as_u64()),
        )?;
        recorder.record_io_step(
            worker.owner,
            resource,
            state_support_bundle_operation(&worker.state),
            format!(
                "domain={} account={} cache_key={} mode={} lane={} closed={}",
                worker.request.domain,
                redact_acme_support_bundle_value(&worker.request.account_id),
                redact_acme_support_bundle_value(&worker.request.cache_key),
                mode_support_bundle_label(worker.request.mode),
                execution_lane_support_bundle_label(&worker.execution_lane),
                worker.closed
            ),
        )
    }

    /// Captures the deterministic replay boundary for renewal/cache/TLS handoff.
    ///
    /// Inputs:
    /// - `handle`: ACME worker with a scheduled deterministic-fixture renewal.
    /// - `recorder`: VM support-bundle replay recorder.
    /// - `tls_listener`: VM TLS listener identity that received the renewed
    ///   certificate handoff.
    ///
    /// Output:
    /// - Three ordered replay steps: renewal metadata, cache handoff, and TLS
    ///   handoff.
    ///
    /// Transformation:
    /// - Converts a completed fixture renewal into a deterministic local replay
    ///   trace without exposing account id, cache key, or certificate material.
    pub(crate) fn capture_deterministic_renewal_cache_tls_handoff_replay(
        &self,
        handle: VmAcmeWorkerHandle,
        recorder: &mut VmSupportBundleReplayRecorder,
        tls_listener: impl Into<String>,
    ) -> Result<Vec<VmSupportBundleReplayStep>, String> {
        let tls_listener = tls_listener.into();
        if tls_listener.trim().is_empty() {
            return Err("ACME replay TLS listener cannot be empty".to_string());
        }

        let worker = self.worker(handle)?;
        let VmAcmeWorkerState::RenewalScheduled {
            not_before_epoch_secs,
        } = worker.state
        else {
            return Err("ACME renewal/cache/TLS replay requires scheduled renewal".to_string());
        };
        let VmAcmeWorkerExecutionLane::DeterministicFixture { fixture_id } = &worker.execution_lane
        else {
            return Err(
                "ACME renewal/cache/TLS replay requires deterministic fixture lane".to_string(),
            );
        };

        let acme_resource = VmSupportBundleReplayResource::new(
            VmSupportBundleReplayResourceKind::AcmeWorker,
            format!("acme-worker:{}", handle.as_u64()),
        )?;
        let tls_resource = VmSupportBundleReplayResource::new(
            VmSupportBundleReplayResourceKind::TlsConnection,
            format!("tls-listener:{tls_listener}"),
        )?;
        let steps = vec![
            recorder.record_io_step(
                worker.owner,
                acme_resource.clone(),
                "acme.renewal.replay.metadata",
                format!(
                    "domain={} fixture={} renew_after={} mode={}",
                    worker.request.domain,
                    fixture_id,
                    not_before_epoch_secs,
                    mode_support_bundle_label(worker.request.mode),
                ),
            )?,
            recorder.record_io_step(
                worker.owner,
                acme_resource,
                "acme.renewal.replay.cache_handoff",
                format!(
                    "cache_key={} atomic_write=true half_written=false",
                    redact_acme_support_bundle_value(&worker.request.cache_key),
                ),
            )?,
            recorder.record_io_step(
                worker.owner,
                tls_resource,
                "acme.renewal.replay.tls_handoff",
                format!(
                    "listener={} replacement_published=true old_connections_preserved=true",
                    tls_listener,
                ),
            )?,
        ];
        Ok(steps)
    }

    fn worker(&self, handle: VmAcmeWorkerHandle) -> Result<&VmAcmeWorker, String> {
        self.workers
            .get(&handle)
            .ok_or_else(|| format!("unknown ACME worker {}", handle.as_u64()))
    }

    fn worker_mut(&mut self, handle: VmAcmeWorkerHandle) -> Result<&mut VmAcmeWorker, String> {
        self.workers
            .get_mut(&handle)
            .ok_or_else(|| format!("unknown ACME worker {}", handle.as_u64()))
    }

    fn enforce_owner_backpressure(&self, owner: VmProcessId) -> Result<(), String> {
        let Some(limit) = self.max_open_workers_per_owner else {
            return Ok(());
        };
        let open_workers = self
            .workers
            .values()
            .filter(|worker| worker.owner == owner && !worker.closed)
            .count();
        if open_workers >= limit {
            Err(format!(
                "ACME worker owner {} reached open worker limit {limit}",
                owner.as_u64()
            ))
        } else {
            Ok(())
        }
    }

    fn record_telemetry_span(
        &mut self,
        handle: VmAcmeWorkerHandle,
        name: &str,
        terminal: bool,
    ) -> Result<(), String> {
        let worker = self.worker_mut(handle)?;
        worker.telemetry_spans.push(VmAcmeWorkerTelemetrySpan {
            worker: handle,
            owner: worker.owner,
            name: name.to_string(),
            domain: worker.request.domain.clone(),
            mode: worker.request.mode,
            terminal,
        });
        Ok(())
    }
}

impl VmAcmeRenewalActor {
    /// Starts a due renewal through the VM-owned ACME worker runtime.
    pub(crate) fn begin_due_renewal(
        &mut self,
        runtime: &mut VmAcmeWorkerRuntime,
        now_epoch_secs: u64,
    ) -> Result<VmAcmeWorkerWake, String> {
        if self.state != VmAcmeRenewalActorState::Waiting {
            return Err("ACME renewal actor is not waiting for renewal".to_string());
        }
        let wake = runtime.begin_due_renewal(self.worker, now_epoch_secs)?;
        self.renewal_timer = None;
        self.state = VmAcmeRenewalActorState::Renewing;
        Ok(wake)
    }

    /// Shuts down the actor and releases its VM-owned timer/resource handle.
    pub(crate) fn shutdown(
        &mut self,
        runtime: &mut VmAcmeWorkerRuntime,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
    ) -> Result<VmAcmeRenewalActorShutdown, String> {
        if self.state == VmAcmeRenewalActorState::Shutdown {
            return Err("ACME renewal actor is already shutdown".to_string());
        }
        let cancelled_timer = match self.renewal_timer.take() {
            Some(timer) => Some(timers.cancel(timer)?),
            None => None,
        };
        if let Some(process) = processes.get_mut(self.owner) {
            process.remove_resource_handle(&renewal_actor_resource_handle(self.worker));
        }
        let terminal_wake = runtime.shutdown_worker(self.worker)?;
        self.state = VmAcmeRenewalActorState::Shutdown;
        Ok(VmAcmeRenewalActorShutdown {
            cancelled_timer,
            terminal_wake,
        })
    }
}

fn validate_execution_lane(lane: &VmAcmeWorkerExecutionLane) -> Result<(), String> {
    match lane {
        VmAcmeWorkerExecutionLane::DeterministicFixture { fixture_id } => {
            if fixture_id.trim().is_empty() {
                Err("ACME deterministic fixture id must not be empty".to_string())
            } else {
                Ok(())
            }
        }
        VmAcmeWorkerExecutionLane::Live { directory_url } => {
            if directory_url.trim().is_empty() {
                return Err("ACME live directory URL must not be empty".to_string());
            }
            if !directory_url.starts_with("https://") {
                return Err("ACME live directory URL must use https".to_string());
            }
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VmAcmeWorkerStateName {
    Requested,
    ChallengeReady,
    Issuing,
    CacheWriting,
    Completed,
}

fn state_name(state: &VmAcmeWorkerState) -> VmAcmeWorkerStateName {
    match state {
        VmAcmeWorkerState::Requested => VmAcmeWorkerStateName::Requested,
        VmAcmeWorkerState::ChallengeReady(_) => VmAcmeWorkerStateName::ChallengeReady,
        VmAcmeWorkerState::Issuing => VmAcmeWorkerStateName::Issuing,
        VmAcmeWorkerState::CacheWriting { .. } => VmAcmeWorkerStateName::CacheWriting,
        VmAcmeWorkerState::RenewalScheduled { .. } => VmAcmeWorkerStateName::Completed,
        VmAcmeWorkerState::Completed => VmAcmeWorkerStateName::Completed,
        VmAcmeWorkerState::Cancelled { .. } => VmAcmeWorkerStateName::Completed,
        VmAcmeWorkerState::Shutdown => VmAcmeWorkerStateName::Completed,
    }
}

fn ensure_state(
    state: &VmAcmeWorkerState,
    allowed: &[VmAcmeWorkerStateName],
) -> Result<(), String> {
    let actual = state_name(state);
    if allowed.contains(&actual) {
        Ok(())
    } else {
        Err(format!(
            "invalid ACME worker state transition from {actual:?}"
        ))
    }
}

fn validate_request(request: &VmAcmeWorkerRequest) -> Result<(), String> {
    if request.domain.trim().is_empty() {
        return Err("ACME worker domain must not be empty".to_string());
    }
    if request.account_id.trim().is_empty() {
        return Err("ACME worker account id must not be empty".to_string());
    }
    if request.cache_key.trim().is_empty() {
        return Err("ACME worker cache key must not be empty".to_string());
    }
    if request.domain.contains("://") || request.domain.contains('/') {
        return Err("ACME worker domain must be a host name, not a URL".to_string());
    }
    Ok(())
}

fn validate_http01_token(token: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("ACME HTTP-01 token must not be empty".to_string());
    }
    if !token
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("ACME HTTP-01 token contains invalid characters".to_string());
    }
    Ok(())
}

fn mode_support_bundle_label(mode: VmAcmeMode) -> &'static str {
    match mode {
        VmAcmeMode::Staging => "staging",
        VmAcmeMode::Live => "live",
    }
}

fn execution_lane_support_bundle_label(lane: &VmAcmeWorkerExecutionLane) -> &'static str {
    match lane {
        VmAcmeWorkerExecutionLane::DeterministicFixture { .. } => "deterministic-fixture",
        VmAcmeWorkerExecutionLane::Live { .. } => "live",
    }
}

fn redact_acme_support_bundle_value(value: &str) -> &'static str {
    if value.is_empty() {
        "empty"
    } else {
        "<redacted>"
    }
}

fn state_support_bundle_operation(state: &VmAcmeWorkerState) -> &'static str {
    match state {
        VmAcmeWorkerState::Requested => "acme.worker.requested",
        VmAcmeWorkerState::ChallengeReady(_) => "acme.worker.challenge_ready",
        VmAcmeWorkerState::Issuing => "acme.worker.issuing",
        VmAcmeWorkerState::CacheWriting { .. } => "acme.worker.cache_writing",
        VmAcmeWorkerState::RenewalScheduled { .. } => "acme.worker.renewal_scheduled",
        VmAcmeWorkerState::Completed => "acme.worker.completed",
        VmAcmeWorkerState::Cancelled { .. } => "acme.worker.cancelled",
        VmAcmeWorkerState::Shutdown => "acme.worker.shutdown",
    }
}

fn renewal_actor_resource_handle(handle: VmAcmeWorkerHandle) -> String {
    format!("acme-renewal-actor:{}", handle.as_u64())
}
