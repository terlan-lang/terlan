use serde_json::{json, Value};

use super::VmHttpQueueMetrics;

/// Measured HTTP work attributed to one VM socket benchmark worker.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct BenchmarkHttpHandlerMetrics {
    pub(super) handler_reductions: usize,
    pub(super) accept_wait_ns: u128,
    pub(super) request_read_parse_ns: u128,
    pub(super) route_match_ns: u128,
    pub(super) request_decode_ns: u128,
    pub(super) handler_run_ns: u128,
    pub(super) synthetic_delay_ns: u128,
    pub(super) response_decode_encode_ns: u128,
    pub(super) response_write_wait_ns: u128,
    pub(super) requests_completed: usize,
    pub(super) connections_closed: usize,
    pub(super) cancellations: usize,
    pub(super) timeouts: usize,
    pub(super) request_read_cancellations: usize,
    pub(super) request_read_timeouts: usize,
    pub(super) response_write_cancellations: usize,
    pub(super) response_write_timeouts: usize,
    pub(super) static_handler_count: usize,
    pub(super) json_handler_count: usize,
    pub(super) add_handler_count: usize,
    pub(super) route_param_handler_count: usize,
    pub(super) stateful_counter_handler_count: usize,
}

impl BenchmarkHttpHandlerMetrics {
    /// Merges one connection or worker sample into this aggregate.
    pub(super) fn add(&mut self, other: Self) {
        self.handler_reductions += other.handler_reductions;
        self.accept_wait_ns += other.accept_wait_ns;
        self.request_read_parse_ns += other.request_read_parse_ns;
        self.route_match_ns += other.route_match_ns;
        self.request_decode_ns += other.request_decode_ns;
        self.handler_run_ns += other.handler_run_ns;
        self.synthetic_delay_ns += other.synthetic_delay_ns;
        self.response_decode_encode_ns += other.response_decode_encode_ns;
        self.response_write_wait_ns += other.response_write_wait_ns;
        self.requests_completed += other.requests_completed;
        self.connections_closed += other.connections_closed;
        self.cancellations += other.cancellations;
        self.timeouts += other.timeouts;
        self.request_read_cancellations += other.request_read_cancellations;
        self.request_read_timeouts += other.request_read_timeouts;
        self.response_write_cancellations += other.response_write_cancellations;
        self.response_write_timeouts += other.response_write_timeouts;
        self.static_handler_count += other.static_handler_count;
        self.json_handler_count += other.json_handler_count;
        self.add_handler_count += other.add_handler_count;
        self.route_param_handler_count += other.route_param_handler_count;
        self.stateful_counter_handler_count += other.stateful_counter_handler_count;
    }

    /// Records one deterministic synthetic handler class by routed path.
    pub(super) fn record_handler_workload(&mut self, path: &str) {
        match path {
            "/synthetic/static" => self.static_handler_count += 1,
            "/synthetic/json" => self.json_handler_count += 1,
            "/add" => self.add_handler_count += 1,
            "/synthetic/users/42" => self.route_param_handler_count += 1,
            "/synthetic/counter" => self.stateful_counter_handler_count += 1,
            _ => {}
        }
    }

    /// Renders aggregate phase and terminal-outcome telemetry for a report.
    pub(super) fn report(metrics: &[Self]) -> Value {
        Self::report_with_scheduler(metrics, &VmHttpQueueMetrics::default(), 0)
    }

    /// Renders runtime phases together with measured scheduler pressure.
    pub(super) fn report_with_scheduler(
        metrics: &[Self],
        queue: &VmHttpQueueMetrics,
        handler_worker_count: usize,
    ) -> Value {
        let aggregate = metrics
            .iter()
            .cloned()
            .fold(Self::default(), |mut all, row| {
                all.add(row);
                all
            });
        let phases = aggregate.phases();
        let (dominant_phase, dominant_ns) = phases
            .iter()
            .max_by_key(|(_, duration)| *duration)
            .copied()
            .unwrap_or(("none", 0));
        let dominant_phase = if dominant_ns == 0 {
            "none"
        } else {
            dominant_phase
        };
        let accounted_total_ns = phases.iter().map(|(_, duration)| duration).sum::<u128>();
        let latency_buckets = aggregate.latency_buckets(queue);
        let phase_bucket_total_ns = latency_buckets
            .iter()
            .filter(|(bucket, _, _)| *bucket != "scheduler")
            .map(|(_, duration, _)| duration)
            .sum::<u128>();
        let (dominant_cause, dominant_cause_ns, dominant_counter) = latency_buckets
            .iter()
            .max_by_key(|(_, duration, _)| *duration)
            .copied()
            .unwrap_or(("none", 0, "none"));
        let (dominant_cause, dominant_counter) = if dominant_cause_ns == 0 {
            ("none", "none")
        } else {
            (dominant_cause, dominant_counter)
        };
        let wakeup_count = queue
            .producer_wakeup_count
            .saturating_add(queue.consumer_wakeup_count);
        let queue_balanced = queue.current_depth == 0 && queue.enqueue_count == queue.dequeue_count;
        let parked_processes_released = queue.parked_producers == 0 && queue.parked_consumers == 0;
        let saturation_has_backpressure_outcome = queue.enqueue_wait_count == 0
            || (queue.enqueue_wait_total_ns > 0 && queue.producer_wakeup_count > 0);
        let classified_handler_count = aggregate.static_handler_count
            + aggregate.json_handler_count
            + aggregate.add_handler_count
            + aggregate.route_param_handler_count
            + aggregate.stateful_counter_handler_count;
        json!({
            "schema": "terlan-vm-http-runtime-attribution-v1",
            "unit": "nanoseconds",
            "includesWarmup": true,
            "requestCount": aggregate.requests_completed,
            "phases": {
                "acceptWaitNs": aggregate.accept_wait_ns,
                "requestReadParseNs": aggregate.request_read_parse_ns,
                "routeMatchNs": aggregate.route_match_ns,
                "requestDecodeNs": aggregate.request_decode_ns,
                "handlerRunNs": aggregate.handler_run_ns,
                "syntheticDelayNs": aggregate.synthetic_delay_ns,
                "responseDecodeEncodeNs": aggregate.response_decode_encode_ns,
                "responseWriteWaitNs": aggregate.response_write_wait_ns
            },
            "accountedTotalNs": accounted_total_ns,
            "dominantBottleneck": {
                "phase": dominant_phase,
                "durationNs": dominant_ns
            },
            "latencyBuckets": {
                "transportNs": aggregate.accept_wait_ns,
                "parserNs": aggregate.request_read_parse_ns,
                "schedulerNs": queue.enqueue_wait_total_ns
                    .saturating_add(queue.dequeue_wait_total_ns),
                "routingNs": aggregate.route_match_ns,
                "allocationAndConversionNs": aggregate.request_decode_ns
                    .saturating_add(aggregate.response_decode_encode_ns),
                "handlerNs": aggregate.handler_run_ns
                    .saturating_add(aggregate.synthetic_delay_ns),
                "responseWriteNs": aggregate.response_write_wait_ns,
                "phaseBucketTotalNs": phase_bucket_total_ns
            },
            "dominantCause": {
                "bucket": dominant_cause,
                "durationNs": dominant_cause_ns,
                "sourceCounter": dominant_counter
            },
            "terminalOutcomes": {
                "completedRequests": aggregate.requests_completed,
                "closedConnections": aggregate.connections_closed,
                "cancellations": aggregate.cancellations,
                "timeouts": aggregate.timeouts,
                "typedReasons": {
                    "client_closed": aggregate.cancellations,
                    "request_timeout": aggregate.timeouts,
                    "client_closed_during_request_read": aggregate.request_read_cancellations,
                    "request_read_timeout": aggregate.request_read_timeouts,
                    "client_closed_during_response_write": aggregate.response_write_cancellations,
                    "response_write_timeout": aggregate.response_write_timeouts
                }
            },
            "handlerWorkloads": {
                "static": aggregate.static_handler_count,
                "json": aggregate.json_handler_count,
                "add": aggregate.add_handler_count,
                "routeParam": aggregate.route_param_handler_count,
                "statefulCounter": aggregate.stateful_counter_handler_count,
                "classifiedRequestCount": classified_handler_count
            },
            "schedulerPressure": {
                "runnableProcessCount": handler_worker_count,
                "parkedProcessCount": queue.parked_producers + queue.parked_consumers,
                "peakParkedProducerCount": queue.max_parked_producers,
                "peakParkedConsumerCount": queue.max_parked_consumers,
                "queueDepth": queue.current_depth,
                "queueMaxDepth": queue.max_depth,
                "queueAdmissionCount": queue.enqueue_count,
                "queueDrainCount": queue.dequeue_count,
                "queueSaturationCount": queue.enqueue_wait_count,
                "backpressureWaitNs": queue.enqueue_wait_total_ns,
                "consumerParkWaitCount": queue.dequeue_wait_count,
                "consumerParkWaitNs": queue.dequeue_wait_total_ns,
                "wakeupCount": wakeup_count,
                "producerWakeupCount": queue.producer_wakeup_count,
                "consumerWakeupCount": queue.consumer_wakeup_count,
                "handlerRetryCount": 0
            },
            "consistency": {
                "completedMatchesReductions": aggregate.requests_completed
                    == aggregate.handler_reductions,
                "phaseBucketsMatchAccountedTotal": phase_bucket_total_ns
                    == accounted_total_ns,
                "queueBalanced": queue_balanced,
                "parkedProcessesReleased": parked_processes_released,
                "saturationHasBackpressureOutcome": saturation_has_backpressure_outcome,
                "classifiedHandlerWorkloadsWithinCompleted": classified_handler_count
                    <= aggregate.requests_completed,
                "terminalOutcomesTyped": true
            }
        })
    }

    /// Returns every exclusive measured request phase for attribution.
    fn phases(&self) -> [(&'static str, u128); 8] {
        [
            ("accept_wait", self.accept_wait_ns),
            ("request_read_parse", self.request_read_parse_ns),
            ("route_match", self.route_match_ns),
            ("request_decode", self.request_decode_ns),
            ("handler_run", self.handler_run_ns),
            ("synthetic_delay", self.synthetic_delay_ns),
            ("response_decode_encode", self.response_decode_encode_ns),
            ("response_write_wait", self.response_write_wait_ns),
        ]
    }

    /// Groups measured phases and scheduler waits into canonical cause buckets.
    fn latency_buckets(
        &self,
        queue: &VmHttpQueueMetrics,
    ) -> [(&'static str, u128, &'static str); 7] {
        [
            ("transport", self.accept_wait_ns, "phases.acceptWaitNs"),
            (
                "parser",
                self.request_read_parse_ns,
                "phases.requestReadParseNs",
            ),
            (
                "scheduler",
                queue
                    .enqueue_wait_total_ns
                    .saturating_add(queue.dequeue_wait_total_ns),
                "schedulerPressure.backpressureWaitNs+consumerParkWaitNs",
            ),
            ("routing", self.route_match_ns, "phases.routeMatchNs"),
            (
                "allocation_conversion",
                self.request_decode_ns
                    .saturating_add(self.response_decode_encode_ns),
                "phases.requestDecodeNs+responseDecodeEncodeNs",
            ),
            (
                "handler",
                self.handler_run_ns.saturating_add(self.synthetic_delay_ns),
                "phases.handlerRunNs+syntheticDelayNs",
            ),
            (
                "response_write",
                self.response_write_wait_ns,
                "phases.responseWriteWaitNs",
            ),
        ]
    }
}

#[cfg(test)]
#[path = "http_attribution_test.rs"]
mod http_attribution_test;
