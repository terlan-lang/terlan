use std::collections::VecDeque;

#[cfg(test)]
use super::memory::{
    VmMemoryAccountant, VmMemoryPressureOutcome, VmSharedAllocationId, VmSharedAllocationKind,
};
use super::native_callable::VmNativeCallableRef;
#[cfg(test)]
use super::process::{VmProcessId, VmProcessTable};
#[cfg(test)]
use super::scheduler::VmScheduler;

#[path = "sse_live_session.rs"]
mod sse_live_session;
#[cfg(test)]
#[path = "sse_test.rs"]
#[cfg(test)]
mod sse_test;
pub(crate) use sse_live_session::VmSseLiveSession;

/// VM SSE stream failure with stable typed variants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSseError {
    #[cfg(test)]
    Closed,
    BackpressureExceeded,
    #[cfg(test)]
    InvalidEventName,
    #[cfg(test)]
    InvalidRetry,
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    InvalidKeepAlive,
    #[cfg(test)]
    HeartbeatTimedOut,
    #[cfg(test)]
    InvalidReconnectToken,
    #[cfg(test)]
    StaleReconnectToken,
    #[cfg(test)]
    InvalidProtocolAssetHash,
    #[cfg(test)]
    StaleProtocolAssetHash,
    #[cfg(test)]
    DomPatchBackpressureExceeded,
    #[cfg(test)]
    EventTooLarge,
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    CallbacksAlreadyConfigured,
}

/// Typed failure from an SSE stream governed by VM memory ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmAccountedSseError {
    Stream(VmSseError),
    Memory(String),
    MemoryPressureRejected,
}

/// Server-sent event envelope owned by the VM HTTP runtime.
///
/// Inputs:
/// - Optional event id, event name, retry hint, and required text data.
///
/// Output:
/// - Typed event that can be encoded into the SSE wire format.
///
/// Transformation:
/// - Keeps handler-facing event metadata explicit instead of asking user code
///   to assemble raw `text/event-stream` frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmSseEvent {
    id: Option<String>,
    event: Option<String>,
    retry_ms: Option<u64>,
    data: String,
}

/// Inspectable SSE stream state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSseStreamInfo {
    pub(crate) pending_events: usize,
    pub(crate) max_pending_events: usize,
    pub(crate) max_event_bytes: usize,
    pub(crate) closed: bool,
    pub(crate) emitted_events: usize,
}

/// Inspectable heartbeat state for a browser-side live-template stream.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseHeartbeatInfo {
    pub(crate) timeout_ms: u64,
    pub(crate) last_seen_ms: u64,
    pub(crate) timed_out: bool,
}

/// VM-owned heartbeat timeout tracker for live SSE protocol streams.
///
/// Inputs:
/// - VM monotonic time in milliseconds, supplied by the scheduler or tests.
///
/// Output:
/// - Deterministic timeout state for stale browser stream detection.
///
/// Transformation:
/// - Keeps heartbeat policy in the VM protocol layer instead of relying on
///   browser glue, host timers, or wall-clock reads inside the handler.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseHeartbeatState {
    timeout_ms: u64,
    last_seen_ms: u64,
    timed_out: bool,
}

/// Inspectable reconnect-token state for a browser-side live-template stream.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseReconnectTokenInfo {
    pub(crate) token: String,
    pub(crate) generation: u64,
    pub(crate) rotated_at_ms: u64,
}

/// VM-owned reconnect token tracker for live SSE protocol streams.
///
/// Inputs:
/// - The token presented by the browser and a replacement token supplied by the
///   VM protocol/session layer.
///
/// Output:
/// - Deterministic token generation state with stale-token rejection.
///
/// Transformation:
/// - Keeps reconnect replay protection in the VM protocol layer instead of
///   letting browser glue or handlers accept stale live-template tokens.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseReconnectTokenState {
    token: String,
    generation: u64,
    rotated_at_ms: u64,
}

/// Inspectable protocol asset hash state for generated live-template assets.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseProtocolAssetHashInfo {
    pub(crate) asset_hash: String,
    pub(crate) generation: u64,
}

/// VM-owned protocol asset hash guard for live SSE browser clients.
///
/// Inputs:
/// - The generated protocol asset hash presented by the browser client.
///
/// Output:
/// - Deterministic accept/reject result for stale live-template assets.
///
/// Transformation:
/// - Keeps hot-reload and rolling-deploy protocol hash compatibility checks in
///   the VM protocol layer instead of letting stale browser bundles apply
///   patches from a different compiler/runtime protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseProtocolAssetHashState {
    asset_hash: String,
    generation: u64,
}

/// Inspectable DOM patch backpressure state for a browser-side live-template stream.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseDomPatchBackpressureInfo {
    pub(crate) pending_patches: usize,
    pub(crate) max_pending_patches: usize,
    pub(crate) applied_patches: usize,
    pub(crate) rejected_patches: usize,
}

/// VM-owned DOM patch application backpressure tracker.
///
/// Inputs:
/// - VM-generated DOM patch ids queued for browser application and browser
///   acknowledgements for patches that finished applying.
///
/// Output:
/// - Deterministic accept/reject result for slow browser patch consumers.
///
/// Transformation:
/// - Keeps live-template DOM patch pressure in the VM protocol layer instead
///   of allowing unbounded browser-side patch lag to accumulate.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSseDomPatchBackpressure {
    pending: VecDeque<String>,
    max_pending_patches: usize,
    applied_patches: usize,
    rejected_patches: usize,
}

/// VM SSE endpoint policy installed on an HTTP route.
///
/// Inputs:
/// - Maximum pending event count, maximum encoded event bytes, and optional
///   keep-alive interval.
///
/// Output:
/// - Route-level SSE policy that can open bounded VM-owned streams.
///
/// Transformation:
/// - Keeps router dispatch typed without storing live mutable stream state in the
///   route table itself.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct VmSseEndpointPlan {
    max_pending_events: usize,
    max_event_bytes: usize,
    keep_alive_ms: Option<u64>,
    callbacks: Option<VmSseCallbackPlan>,
}

/// Complete static callback set for one generated SSE endpoint.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct VmSseCallbackPlan {
    /// Called after stream admission.
    pub(crate) open: VmNativeCallableRef,
    /// Called when one application event becomes ready.
    pub(crate) event_ready: VmNativeCallableRef,
    /// Called when the VM emits a keep-alive comment.
    pub(crate) keep_alive: VmNativeCallableRef,
    /// Called before graceful stream drain and close.
    pub(crate) drain: VmNativeCallableRef,
    /// Called during abrupt scheduler or transport cancellation.
    pub(crate) cancellation: VmNativeCallableRef,
}

/// VM-owned bounded SSE stream queue.
///
/// Inputs:
/// - Typed SSE events from handlers or actors.
///
/// Output:
/// - Encoded SSE frames drained by HTTP streaming code.
///
/// Transformation:
/// - Applies deterministic backpressure and close semantics without exposing
///   host async runtime primitives or unbounded buffers.
#[derive(Debug)]
pub(crate) struct VmSseStream {
    pending: VecDeque<VmSseEvent>,
    max_pending_events: usize,
    max_event_bytes: usize,
    closed: bool,
    emitted_events: usize,
}

/// SSE stream whose queued protocol buffers are owned by one VM process.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VmAccountedSseStream {
    stream: VmSseStream,
    owner: VmProcessId,
    allocations: VecDeque<VmSharedAllocationId>,
}

impl VmSseEvent {
    /// Creates a data-only SSE event.
    #[cfg(test)]
    pub(crate) fn data(data: impl Into<String>) -> Self {
        Self {
            id: None,
            event: None,
            retry_ms: None,
            data: data.into(),
        }
    }

    /// Adds an event id.
    #[cfg(test)]
    pub(crate) fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Adds an event name.
    #[cfg(test)]
    pub(crate) fn with_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Adds a reconnect retry hint in milliseconds.
    #[cfg(test)]
    pub(crate) fn with_retry_ms(mut self, retry_ms: u64) -> Self {
        self.retry_ms = Some(retry_ms);
        self
    }

    /// Encodes this event into the SSE wire format.
    #[cfg(test)]
    pub(crate) fn encode(&self) -> Result<Vec<u8>, VmSseError> {
        let text = encode_event_text(self)?;
        Ok(text.into_bytes())
    }
}

impl VmSseEndpointPlan {
    /// Creates an SSE endpoint plan with explicit non-zero stream limits.
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(crate) fn new(
        max_pending_events: usize,
        max_event_bytes: usize,
    ) -> Result<Self, VmSseError> {
        if max_pending_events == 0 || max_event_bytes == 0 {
            return Err(VmSseError::BackpressureExceeded);
        }
        Ok(Self {
            max_pending_events,
            max_event_bytes,
            keep_alive_ms: None,
            callbacks: None,
        })
    }

    /// Adds a non-zero keep-alive interval in milliseconds.
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(crate) fn with_keep_alive_ms(mut self, keep_alive_ms: u64) -> Result<Self, VmSseError> {
        if keep_alive_ms == 0 {
            return Err(VmSseError::InvalidKeepAlive);
        }
        self.keep_alive_ms = Some(keep_alive_ms);
        Ok(self)
    }

    /// Opens a bounded stream instance from this route endpoint plan.
    pub(crate) fn open_stream(&self) -> Result<VmSseStream, VmSseError> {
        VmSseStream::new(self.max_pending_events, self.max_event_bytes)
    }

    /// Returns the maximum queued event count for streams opened from this plan.
    pub(crate) fn max_pending_events(&self) -> usize {
        self.max_pending_events
    }

    /// Returns the maximum encoded event size for streams opened from this plan.
    #[cfg(test)]
    pub(crate) fn max_event_bytes(&self) -> usize {
        self.max_event_bytes
    }

    /// Returns the optional keep-alive interval in milliseconds.
    #[cfg(test)]
    pub(crate) fn keep_alive_ms(&self) -> Option<u64> {
        self.keep_alive_ms
    }

    /// Attaches one complete closure-free callback set to this endpoint.
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(crate) fn with_callbacks(
        mut self,
        callbacks: VmSseCallbackPlan,
    ) -> Result<Self, VmSseError> {
        if self.callbacks.is_some() {
            return Err(VmSseError::CallbacksAlreadyConfigured);
        }
        self.callbacks = Some(callbacks);
        Ok(self)
    }

    /// Returns the generated callback set retained by this endpoint.
    pub(crate) fn callbacks(&self) -> Option<&VmSseCallbackPlan> {
        self.callbacks.as_ref()
    }
}

#[cfg(test)]
impl VmSseHeartbeatState {
    /// Creates heartbeat state with a non-zero timeout and initial observation.
    pub(crate) fn new(timeout_ms: u64, now_ms: u64) -> Result<Self, VmSseError> {
        if timeout_ms == 0 {
            return Err(VmSseError::InvalidKeepAlive);
        }
        Ok(Self {
            timeout_ms,
            last_seen_ms: now_ms,
            timed_out: false,
        })
    }

    /// Records a heartbeat and clears previous timeout state.
    pub(crate) fn record_heartbeat(&mut self, now_ms: u64) {
        self.last_seen_ms = now_ms;
        self.timed_out = false;
    }

    /// Evaluates whether the heartbeat has exceeded its allowed idle window.
    pub(crate) fn evaluate_timeout(&mut self, now_ms: u64) -> Result<(), VmSseError> {
        if now_ms.saturating_sub(self.last_seen_ms) > self.timeout_ms {
            self.timed_out = true;
            return Err(VmSseError::HeartbeatTimedOut);
        }
        Ok(())
    }

    /// Returns inspectable heartbeat timeout state.
    pub(crate) fn inspect(&self) -> VmSseHeartbeatInfo {
        VmSseHeartbeatInfo {
            timeout_ms: self.timeout_ms,
            last_seen_ms: self.last_seen_ms,
            timed_out: self.timed_out,
        }
    }
}

#[cfg(test)]
impl VmSseReconnectTokenState {
    /// Creates reconnect-token state with a non-empty initial token.
    pub(crate) fn new(initial_token: impl Into<String>, now_ms: u64) -> Result<Self, VmSseError> {
        let token = initial_token.into();
        validate_reconnect_token(&token)?;
        Ok(Self {
            token,
            generation: 0,
            rotated_at_ms: now_ms,
        })
    }

    /// Returns the token currently accepted for reconnect attempts.
    pub(crate) fn current_token(&self) -> &str {
        &self.token
    }

    /// Rotates the reconnect token after validating the browser-presented token.
    pub(crate) fn rotate(
        &mut self,
        presented_token: &str,
        next_token: impl Into<String>,
        now_ms: u64,
    ) -> Result<VmSseReconnectTokenInfo, VmSseError> {
        validate_reconnect_token(presented_token)?;
        if presented_token != self.token {
            return Err(VmSseError::StaleReconnectToken);
        }

        let next_token = next_token.into();
        validate_reconnect_token(&next_token)?;
        if next_token == self.token {
            return Err(VmSseError::InvalidReconnectToken);
        }

        self.token = next_token;
        self.generation += 1;
        self.rotated_at_ms = now_ms;
        Ok(self.inspect())
    }

    /// Returns inspectable reconnect token state.
    pub(crate) fn inspect(&self) -> VmSseReconnectTokenInfo {
        VmSseReconnectTokenInfo {
            token: self.token.clone(),
            generation: self.generation,
            rotated_at_ms: self.rotated_at_ms,
        }
    }
}

#[cfg(test)]
impl VmSseProtocolAssetHashState {
    /// Creates protocol asset hash state with a non-empty hash.
    pub(crate) fn new(asset_hash: impl Into<String>) -> Result<Self, VmSseError> {
        let asset_hash = asset_hash.into();
        validate_protocol_asset_hash(&asset_hash)?;
        Ok(Self {
            asset_hash,
            generation: 0,
        })
    }

    /// Validates that the browser-presented protocol asset hash is current.
    pub(crate) fn validate_presented_hash(&self, presented_hash: &str) -> Result<(), VmSseError> {
        validate_protocol_asset_hash(presented_hash)?;
        if presented_hash != self.asset_hash {
            return Err(VmSseError::StaleProtocolAssetHash);
        }
        Ok(())
    }

    /// Replaces the active asset hash after a compatible rebuild or hot reload.
    pub(crate) fn replace_hash(&mut self, next_hash: impl Into<String>) -> Result<(), VmSseError> {
        let next_hash = next_hash.into();
        validate_protocol_asset_hash(&next_hash)?;
        if next_hash != self.asset_hash {
            self.asset_hash = next_hash;
            self.generation += 1;
        }
        Ok(())
    }

    /// Returns inspectable protocol asset hash state.
    pub(crate) fn inspect(&self) -> VmSseProtocolAssetHashInfo {
        VmSseProtocolAssetHashInfo {
            asset_hash: self.asset_hash.clone(),
            generation: self.generation,
        }
    }
}

#[cfg(test)]
impl VmSseDomPatchBackpressure {
    /// Creates DOM patch backpressure state with a non-zero pending patch limit.
    pub(crate) fn new(max_pending_patches: usize) -> Result<Self, VmSseError> {
        if max_pending_patches == 0 {
            return Err(VmSseError::DomPatchBackpressureExceeded);
        }
        Ok(Self {
            pending: VecDeque::new(),
            max_pending_patches,
            applied_patches: 0,
            rejected_patches: 0,
        })
    }

    /// Queues a DOM patch id unless the browser-side application queue is full.
    pub(crate) fn queue_patch(&mut self, patch_id: impl Into<String>) -> Result<(), VmSseError> {
        if self.pending.len() >= self.max_pending_patches {
            self.rejected_patches += 1;
            return Err(VmSseError::DomPatchBackpressureExceeded);
        }
        self.pending.push_back(patch_id.into());
        Ok(())
    }

    /// Acknowledges the oldest browser-applied DOM patch.
    pub(crate) fn acknowledge_applied_patch(&mut self) -> Option<String> {
        let patch_id = self.pending.pop_front()?;
        self.applied_patches += 1;
        Some(patch_id)
    }

    /// Returns inspectable DOM patch backpressure state.
    pub(crate) fn inspect(&self) -> VmSseDomPatchBackpressureInfo {
        VmSseDomPatchBackpressureInfo {
            pending_patches: self.pending.len(),
            max_pending_patches: self.max_pending_patches,
            applied_patches: self.applied_patches,
            rejected_patches: self.rejected_patches,
        }
    }
}

impl VmSseStream {
    /// Creates a bounded SSE stream.
    pub(crate) fn new(
        max_pending_events: usize,
        max_event_bytes: usize,
    ) -> Result<Self, VmSseError> {
        if max_pending_events == 0 || max_event_bytes == 0 {
            return Err(VmSseError::BackpressureExceeded);
        }
        Ok(Self {
            pending: VecDeque::new(),
            max_pending_events,
            max_event_bytes,
            closed: false,
            emitted_events: 0,
        })
    }

    /// Enqueues one typed event after validating size and stream state.
    #[cfg(test)]
    pub(crate) fn enqueue(&mut self, event: VmSseEvent) -> Result<(), VmSseError> {
        self.validate_enqueue(&event)?;
        self.pending.push_back(event);
        Ok(())
    }

    #[cfg(test)]
    fn validate_enqueue(&self, event: &VmSseEvent) -> Result<usize, VmSseError> {
        if self.closed {
            return Err(VmSseError::Closed);
        }
        if self.pending.len() >= self.max_pending_events {
            return Err(VmSseError::BackpressureExceeded);
        }
        let encoded_bytes = event.encode()?.len();
        if encoded_bytes > self.max_event_bytes {
            return Err(VmSseError::EventTooLarge);
        }
        Ok(encoded_bytes)
    }

    /// Encodes and removes the next queued SSE event.
    #[cfg(test)]
    pub(crate) fn flush_next(&mut self) -> Result<Option<Vec<u8>>, VmSseError> {
        let Some(event) = self.pending.pop_front() else {
            return Ok(None);
        };
        self.emitted_events += 1;
        Ok(Some(event.encode()?))
    }

    /// Closes the stream for new events while retaining already queued events.
    #[cfg(test)]
    pub(crate) fn close(&mut self) {
        self.closed = true;
    }

    /// Returns an inspectable stream snapshot.
    pub(crate) fn inspect(&self) -> VmSseStreamInfo {
        VmSseStreamInfo {
            pending_events: self.pending.len(),
            max_pending_events: self.max_pending_events,
            max_event_bytes: self.max_event_bytes,
            closed: self.closed,
            emitted_events: self.emitted_events,
        }
    }
}

#[cfg(test)]
impl VmAccountedSseStream {
    /// Opens a bounded SSE queue owned by one live VM process.
    pub(crate) fn new(
        owner: VmProcessId,
        max_pending_events: usize,
        max_event_bytes: usize,
    ) -> Result<Self, VmSseError> {
        Ok(Self {
            stream: VmSseStream::new(max_pending_events, max_event_bytes)?,
            owner,
            allocations: VecDeque::new(),
        })
    }

    /// Queues one event only after reserving its encoded protocol-buffer bytes.
    pub(crate) fn enqueue(
        &mut self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
        event: VmSseEvent,
    ) -> Result<(), VmAccountedSseError> {
        let encoded_bytes = self
            .stream
            .validate_enqueue(&event)
            .map_err(VmAccountedSseError::Stream)?;
        let decision = memory
            .register_shared_allocation(
                processes,
                self.owner,
                VmSharedAllocationKind::ProtocolBuffer,
                encoded_bytes,
            )
            .map_err(VmAccountedSseError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, self.owner, encoded_bytes)
            .map_err(VmAccountedSseError::Memory)?;
        if decision.pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Err(VmAccountedSseError::MemoryPressureRejected);
        }
        let allocation = decision.allocation_id.ok_or_else(|| {
            VmAccountedSseError::Memory(
                "accounted SSE enqueue did not produce an allocation id".to_string(),
            )
        })?;
        self.stream.pending.push_back(event);
        self.allocations.push_back(allocation);
        Ok(())
    }

    /// Flushes one event and releases its exact protocol-buffer ownership.
    pub(crate) fn flush_next(
        &mut self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
    ) -> Result<Option<Vec<u8>>, VmAccountedSseError> {
        let Some(event) = self.stream.pending.front() else {
            return Ok(None);
        };
        let encoded = event.encode().map_err(VmAccountedSseError::Stream)?;
        let allocation = self.allocations.front().copied().ok_or_else(|| {
            VmAccountedSseError::Memory(
                "accounted SSE queue is missing protocol-buffer ownership".to_string(),
            )
        })?;
        memory
            .release_shared_allocation(processes, allocation, self.owner)
            .map_err(VmAccountedSseError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, self.owner, encoded.len())
            .map_err(VmAccountedSseError::Memory)?;
        self.stream.pending.pop_front();
        self.allocations.pop_front();
        self.stream.emitted_events += 1;
        Ok(Some(encoded))
    }

    /// Cancels the stream and atomically releases every pending protocol buffer.
    pub(crate) fn cancel(
        &mut self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
    ) -> Result<usize, VmAccountedSseError> {
        let released_bytes = self
            .stream
            .pending
            .iter()
            .try_fold(0usize, |total, event| {
                let bytes = event.encode().map_err(VmAccountedSseError::Stream)?.len();
                total.checked_add(bytes).ok_or_else(|| {
                    VmAccountedSseError::Memory(
                        "accounted SSE cancellation byte size overflow".to_string(),
                    )
                })
            })?;
        let allocations = self.allocations.iter().copied().collect::<Vec<_>>();
        let released = memory
            .release_shared_allocations(processes, &allocations, self.owner)
            .map_err(VmAccountedSseError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, self.owner, released_bytes)
            .map_err(VmAccountedSseError::Memory)?;
        self.allocations.clear();
        self.stream.pending.clear();
        self.stream.close();
        Ok(released)
    }

    pub(crate) fn inspect(&self) -> VmSseStreamInfo {
        self.stream.inspect()
    }
}

/// Encodes a comment keep-alive frame.
#[cfg(test)]
pub(crate) fn keep_alive_frame() -> Vec<u8> {
    b": keep-alive\n\n".to_vec()
}

/// Builds one Rust-backed SSE event value for `std.http.Sse.data`.
#[cfg(test)]
pub fn data(value: String) -> VmSseEvent {
    VmSseEvent::data(value)
}

/// Builds one Rust-backed SSE event value with an id.
#[cfg(test)]
pub fn with_id(event: VmSseEvent, id: String) -> VmSseEvent {
    event.with_id(id)
}

/// Builds one Rust-backed SSE event value with an event name.
#[cfg(test)]
pub fn with_name(event: VmSseEvent, name: String) -> VmSseEvent {
    event.with_event(name)
}

/// Builds one Rust-backed SSE event value with a retry hint.
#[cfg(test)]
pub fn with_retry_ms(event: VmSseEvent, retry_ms: u64) -> VmSseEvent {
    event.with_retry_ms(retry_ms)
}

/// Returns queued SSE response inputs after validating event encodability.
#[cfg(test)]
pub fn response(
    events: Vec<VmSseEvent>,
    status: u16,
) -> Result<(Vec<VmSseEvent>, u16), VmSseError> {
    for event in &events {
        event.encode()?;
    }
    Ok((events, status))
}

/// Builds one Rust-backed SSE endpoint plan.
#[cfg(test)]
pub fn endpoint(
    max_pending_events: usize,
    max_event_bytes: usize,
) -> Result<VmSseEndpointPlan, VmSseError> {
    VmSseEndpointPlan::new(max_pending_events, max_event_bytes)
}

/// Builds one Rust-backed SSE endpoint plan with a keep-alive interval.
#[cfg(test)]
pub fn endpoint_with_keep_alive(
    max_pending_events: usize,
    max_event_bytes: usize,
    keep_alive_ms: u64,
) -> Result<VmSseEndpointPlan, VmSseError> {
    VmSseEndpointPlan::new(max_pending_events, max_event_bytes)?.with_keep_alive_ms(keep_alive_ms)
}

#[cfg(test)]
fn encode_event_text(event: &VmSseEvent) -> Result<String, VmSseError> {
    validate_line_field(event.id.as_deref()).map_err(|_| VmSseError::InvalidEventName)?;
    validate_line_field(event.event.as_deref()).map_err(|_| VmSseError::InvalidEventName)?;
    if event.retry_ms == Some(0) {
        return Err(VmSseError::InvalidRetry);
    }

    let mut output = String::new();
    if let Some(id) = &event.id {
        output.push_str("id: ");
        output.push_str(id);
        output.push('\n');
    }
    if let Some(event_name) = &event.event {
        output.push_str("event: ");
        output.push_str(event_name);
        output.push('\n');
    }
    if let Some(retry_ms) = event.retry_ms {
        output.push_str("retry: ");
        output.push_str(&retry_ms.to_string());
        output.push('\n');
    }
    for line in event.data.lines() {
        output.push_str("data: ");
        output.push_str(line);
        output.push('\n');
    }
    if event.data.ends_with('\n') {
        output.push_str("data: \n");
    }
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
fn validate_line_field(value: Option<&str>) -> Result<(), ()> {
    match value {
        Some(value) if value.contains('\n') || value.contains('\r') || value.contains('\0') => {
            Err(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
fn validate_reconnect_token(token: &str) -> Result<(), VmSseError> {
    if token.trim().is_empty()
        || token
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(VmSseError::InvalidReconnectToken);
    }
    Ok(())
}

#[cfg(test)]
fn validate_protocol_asset_hash(asset_hash: &str) -> Result<(), VmSseError> {
    if asset_hash.trim().is_empty()
        || asset_hash
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(VmSseError::InvalidProtocolAssetHash);
    }
    Ok(())
}
