use super::BenchmarkHttpHandlerMetrics;
use crate::runtime::vm::http::VmHttpQueueMetrics;

#[test]
fn runtime_attribution_aggregates_phases_and_classifies_dominant_bottleneck() {
    let report = BenchmarkHttpHandlerMetrics::report(&[
        BenchmarkHttpHandlerMetrics {
            handler_reductions: 2,
            request_read_parse_ns: 20,
            handler_run_ns: 200,
            response_write_wait_ns: 40,
            requests_completed: 2,
            connections_closed: 1,
            ..BenchmarkHttpHandlerMetrics::default()
        },
        BenchmarkHttpHandlerMetrics {
            handler_reductions: 1,
            request_read_parse_ns: 10,
            handler_run_ns: 100,
            response_write_wait_ns: 20,
            requests_completed: 1,
            connections_closed: 1,
            ..BenchmarkHttpHandlerMetrics::default()
        },
    ]);

    assert_eq!(report["requestCount"], 3);
    assert_eq!(report["phases"]["requestReadParseNs"], 30);
    assert_eq!(report["phases"]["handlerRunNs"], 300);
    assert_eq!(report["dominantBottleneck"]["phase"], "handler_run");
    assert_eq!(report["dominantBottleneck"]["durationNs"], 300);
    assert_eq!(report["terminalOutcomes"]["closedConnections"], 2);
    assert!(report["consistency"]["completedMatchesReductions"]
        .as_bool()
        .expect("completion consistency"));
}

#[test]
fn runtime_attribution_exposes_inconsistent_completion_accounting() {
    let report = BenchmarkHttpHandlerMetrics::report(&[BenchmarkHttpHandlerMetrics {
        handler_reductions: 2,
        requests_completed: 1,
        cancellations: 1,
        ..BenchmarkHttpHandlerMetrics::default()
    }]);

    assert_eq!(report["terminalOutcomes"]["cancellations"], 1);
    assert!(!report["consistency"]["completedMatchesReductions"]
        .as_bool()
        .expect("completion consistency"));
}

#[test]
fn runtime_attribution_preserves_typed_terminal_stage_reasons() {
    let report = BenchmarkHttpHandlerMetrics::report(&[BenchmarkHttpHandlerMetrics {
        cancellations: 3,
        timeouts: 4,
        request_read_cancellations: 1,
        request_read_timeouts: 2,
        response_write_cancellations: 2,
        response_write_timeouts: 2,
        ..BenchmarkHttpHandlerMetrics::default()
    }]);
    let reasons = &report["terminalOutcomes"]["typedReasons"];

    assert_eq!(reasons["client_closed"], 3);
    assert_eq!(reasons["request_timeout"], 4);
    assert_eq!(reasons["client_closed_during_request_read"], 1);
    assert_eq!(reasons["request_read_timeout"], 2);
    assert_eq!(reasons["client_closed_during_response_write"], 2);
    assert_eq!(reasons["response_write_timeout"], 2);
}

#[test]
fn runtime_attribution_reports_scheduler_pressure_and_consistency() {
    let report = BenchmarkHttpHandlerMetrics::report_with_scheduler(
        &[BenchmarkHttpHandlerMetrics {
            handler_reductions: 2,
            requests_completed: 2,
            ..BenchmarkHttpHandlerMetrics::default()
        }],
        &VmHttpQueueMetrics {
            max_depth: 2,
            enqueue_count: 2,
            dequeue_count: 2,
            enqueue_wait_count: 1,
            enqueue_wait_total_ns: 700,
            dequeue_wait_count: 1,
            dequeue_wait_total_ns: 300,
            max_parked_producers: 1,
            max_parked_consumers: 1,
            producer_wakeup_count: 1,
            consumer_wakeup_count: 1,
            ..VmHttpQueueMetrics::default()
        },
        2,
    );

    assert_eq!(report["schedulerPressure"]["runnableProcessCount"], 2);
    assert_eq!(report["schedulerPressure"]["queueSaturationCount"], 1);
    assert_eq!(report["schedulerPressure"]["backpressureWaitNs"], 700);
    assert_eq!(report["schedulerPressure"]["wakeupCount"], 2);
    assert_eq!(report["schedulerPressure"]["handlerRetryCount"], 0);
    assert!(report["consistency"]["queueBalanced"]
        .as_bool()
        .expect("queue consistency"));
    assert!(report["consistency"]["parkedProcessesReleased"]
        .as_bool()
        .expect("parked process consistency"));
    assert!(report["consistency"]["saturationHasBackpressureOutcome"]
        .as_bool()
        .expect("saturation consistency"));
}

#[test]
fn runtime_attribution_rejects_unexplained_scheduler_saturation() {
    let report = BenchmarkHttpHandlerMetrics::report_with_scheduler(
        &[],
        &VmHttpQueueMetrics {
            current_depth: 1,
            enqueue_count: 2,
            dequeue_count: 1,
            enqueue_wait_count: 1,
            ..VmHttpQueueMetrics::default()
        },
        1,
    );

    assert!(!report["consistency"]["queueBalanced"]
        .as_bool()
        .expect("queue consistency"));
    assert!(!report["consistency"]["saturationHasBackpressureOutcome"]
        .as_bool()
        .expect("saturation consistency"));
}

#[test]
fn runtime_attribution_buckets_every_measured_phase_once() {
    let report = BenchmarkHttpHandlerMetrics::report_with_scheduler(
        &[BenchmarkHttpHandlerMetrics {
            accept_wait_ns: 10,
            request_read_parse_ns: 20,
            route_match_ns: 30,
            request_decode_ns: 40,
            handler_run_ns: 50,
            synthetic_delay_ns: 60,
            response_decode_encode_ns: 70,
            response_write_wait_ns: 80,
            ..BenchmarkHttpHandlerMetrics::default()
        }],
        &VmHttpQueueMetrics {
            enqueue_wait_total_ns: 90,
            dequeue_wait_total_ns: 100,
            ..VmHttpQueueMetrics::default()
        },
        1,
    );

    assert_eq!(report["latencyBuckets"]["transportNs"], 10);
    assert_eq!(report["latencyBuckets"]["parserNs"], 20);
    assert_eq!(report["latencyBuckets"]["schedulerNs"], 190);
    assert_eq!(report["latencyBuckets"]["routingNs"], 30);
    assert_eq!(report["latencyBuckets"]["allocationAndConversionNs"], 110);
    assert_eq!(report["latencyBuckets"]["handlerNs"], 110);
    assert_eq!(report["latencyBuckets"]["responseWriteNs"], 80);
    assert_eq!(report["latencyBuckets"]["phaseBucketTotalNs"], 360);
    assert_eq!(report["accountedTotalNs"], 360);
    assert!(report["consistency"]["phaseBucketsMatchAccountedTotal"]
        .as_bool()
        .expect("latency bucket consistency"));
}

#[test]
fn runtime_attribution_classifies_scheduler_as_dominant_cause() {
    let report = BenchmarkHttpHandlerMetrics::report_with_scheduler(
        &[BenchmarkHttpHandlerMetrics {
            handler_run_ns: 100,
            ..BenchmarkHttpHandlerMetrics::default()
        }],
        &VmHttpQueueMetrics {
            enqueue_wait_total_ns: 400,
            dequeue_wait_total_ns: 200,
            ..VmHttpQueueMetrics::default()
        },
        1,
    );

    assert_eq!(report["dominantCause"]["bucket"], "scheduler");
    assert_eq!(report["dominantCause"]["durationNs"], 600);
    assert_eq!(
        report["dominantCause"]["sourceCounter"],
        "schedulerPressure.backpressureWaitNs+consumerParkWaitNs"
    );
}

#[test]
fn runtime_attribution_classifies_deterministic_handler_workloads() {
    let mut metrics = BenchmarkHttpHandlerMetrics {
        handler_reductions: 5,
        requests_completed: 5,
        ..BenchmarkHttpHandlerMetrics::default()
    };
    for path in [
        "/synthetic/static",
        "/synthetic/json",
        "/add",
        "/synthetic/users/42",
        "/synthetic/counter",
    ] {
        metrics.record_handler_workload(path);
    }

    let report = BenchmarkHttpHandlerMetrics::report(&[metrics]);

    assert_eq!(report["handlerWorkloads"]["static"], 1);
    assert_eq!(report["handlerWorkloads"]["json"], 1);
    assert_eq!(report["handlerWorkloads"]["add"], 1);
    assert_eq!(report["handlerWorkloads"]["routeParam"], 1);
    assert_eq!(report["handlerWorkloads"]["statefulCounter"], 1);
    assert_eq!(report["handlerWorkloads"]["classifiedRequestCount"], 5);
    assert!(
        report["consistency"]["classifiedHandlerWorkloadsWithinCompleted"]
            .as_bool()
            .expect("handler workload consistency")
    );
}
