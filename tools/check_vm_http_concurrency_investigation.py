#!/usr/bin/env python3
"""Validate VM HTTP concurrency investigation artifacts.

Inputs:
- `../benchmarks/results/http-vm-vs-rust.latest.json`.
- `../benchmarks/reports/http-vm-vs-rust.latest.md`.

Outputs:
- Exit status 0 when the comparison report has the required concurrency,
  keep-alive, computed-handler, evidence-classification, and stable platform
  skip fields.
- Exit status 1 with stable diagnostics for missing or malformed rows.

Transformation:
- Keeps the 0.0.7 VM HTTP concurrency investigation executable without
  requiring every developer sandbox to permit loopback listener binding.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_ROOT = ROOT.parent
COMPARISON_JSON = WORKSPACE_ROOT / "benchmarks" / "results" / "http-vm-vs-rust.latest.json"
COMPARISON_MD = WORKSPACE_ROOT / "benchmarks" / "reports" / "http-vm-vs-rust.latest.md"
TERLAN_LANE = "vm_http_socket_http1_vm_owned_async"

REQUIRED_REALISM_DIMENSIONS = {
    "async_io",
    "fairness",
    "backpressure",
    "protocol_parsing",
    "connection_lifecycle",
    "ecosystem_integration",
    "long_running_load",
}

REALISM_STATUSES = {"covered", "partial", "missing", "not-applicable"}
STATISTICAL_CREDIBILITY_CLASSES = {
    "directional-single-run",
    "statistically-credible",
    "not-required",
}
STATISTICAL_REQUIRED_METRICS = {
    "mean_us",
    "p99_us",
    "wall_total_us",
    "throughput_requests_per_second",
}
SUSTAINED_REQUIRED_SAMPLE_COUNT = 3

REQUIRED_GROUP_EVIDENCE = {
    "socket-handler-c1": "advisory-single-request",
    "socket-handler-c100": "advisory-one-request-socket",
    "socket-handler-c1000": "advisory-one-request-churn",
    "socket-keepalive-c100": "sustained-keepalive",
    "socket-keepalive-c1000": "sustained-keepalive",
    "socket-crud-c100": "advisory-one-request-socket",
    "socket-crud-c1000": "advisory-one-request-churn",
    "socket-add-c100": "advisory-one-request-socket",
    "socket-add-c1000": "advisory-one-request-churn",
    "socket-add-keepalive-c100": "sustained-keepalive",
    "socket-add-keepalive-c1000": "sustained-keepalive",
    "socket-payload-4096-c100": "advisory-one-request-socket",
    "socket-payload-4096-c1000": "advisory-one-request-churn",
}

REQUIRED_HYPER_GROUP_EVIDENCE = {
    "socket-handler-c100-hyper": "advisory-one-request-socket",
    "socket-handler-c1000-hyper": "advisory-one-request-churn",
    "socket-keepalive-c100-hyper": "sustained-keepalive",
    "socket-keepalive-c1000-hyper": "sustained-keepalive",
    "socket-add-c100-hyper": "advisory-one-request-socket",
    "socket-add-c1000-hyper": "advisory-one-request-churn",
    "socket-add-keepalive-c100-hyper": "sustained-keepalive",
    "socket-add-keepalive-c1000-hyper": "sustained-keepalive",
}

REQUIRED_COWBOY_GROUP_EVIDENCE = {
    "socket-add-keepalive-c100-cowboy": "sustained-keepalive",
    "socket-add-keepalive-c1000-cowboy": "sustained-keepalive",
}

REQUIRED_TERLAN_REPORT_KEYS = {
    "handler",
    "stack",
    "vm-stream",
    "socket-1",
    "socket-100",
    "socket-1000",
    "socket-100-keepalive",
    "socket-1000-keepalive",
    "socket-100-crud",
    "socket-1000-crud",
    "socket-100-add",
    "socket-1000-add",
    "socket-100-add-keepalive",
    "socket-1000-add-keepalive",
    "socket-100-payload-4096",
    "socket-1000-payload-4096",
}

SOCKET_REPORT_REQUIRED_INT_FIELDS = (
    "concurrency",
    "effective_concurrency",
    "queue_capacity",
    "acceptor_count",
    "handler_worker_count",
    "requests_per_connection",
    "connection_count",
)

SOCKET_QUEUE_PRESSURE_REQUIRED_INT_FIELDS = (
    "max_depth",
    "enqueue_wait_count",
    "enqueue_wait_total_us",
)


def main() -> int:
    """Validate the current VM HTTP concurrency comparison report."""

    if "--self-test" in sys.argv[1:]:
        return run_self_test()
    diagnostics = validate_report()
    if diagnostics:
        for diagnostic in diagnostics:
            print(diagnostic, file=sys.stderr)
        return 1
    print("[vm-http-concurrency-investigation-check] artifacts validated")
    return 0


def validate_report() -> list[str]:
    """Return diagnostics for malformed benchmark comparison artifacts."""

    diagnostics: list[str] = []
    payload = read_json_object(COMPARISON_JSON, diagnostics)
    markdown = read_text(COMPARISON_MD, diagnostics)
    if payload is None:
        return diagnostics
    if payload.get("benchmark") != "http-vm-vs-rust":
        diagnostics.append(f"{COMPARISON_JSON}: benchmark must be http-vm-vs-rust")
    if payload.get("status") != "completed":
        diagnostics.append(f"{COMPARISON_JSON}: status must be completed")
    if not isinstance(payload.get("rust_http_platform_skipped"), bool):
        diagnostics.append(f"{COMPARISON_JSON}: rust_http_platform_skipped must be boolean")
    validate_cowboy_report_reference(payload, diagnostics)
    validate_terlan_report_keys(payload, diagnostics)
    validate_terlan_socket_report_metadata(payload, diagnostics)
    lanes = lanes_by_group(payload, diagnostics)
    validate_group_evidence(lanes, REQUIRED_GROUP_EVIDENCE, diagnostics)
    validate_group_evidence(lanes, REQUIRED_HYPER_GROUP_EVIDENCE, diagnostics)
    validate_group_evidence(lanes, REQUIRED_COWBOY_GROUP_EVIDENCE, diagnostics)
    validate_cowboy_lanes(lanes, diagnostics)
    validate_required_metrics(lanes, diagnostics)
    validate_http_realism_matrix(payload, lanes, diagnostics)
    validate_repeated_run_summary(payload, lanes, diagnostics)
    validate_statistical_credibility_matrix(payload, lanes, diagnostics)
    validate_performance_clue_reports(payload, lanes, diagnostics)
    validate_markdown(markdown, diagnostics)
    return diagnostics


def read_json_object(path: Path, diagnostics: list[str]) -> dict[str, Any] | None:
    """Read a JSON object and append stable diagnostics on failure."""

    if not path.is_file():
        diagnostics.append(f"{path}: missing comparison JSON")
        return None
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        diagnostics.append(f"{path}: invalid JSON: {error}")
        return None
    if not isinstance(value, dict):
        diagnostics.append(f"{path}: expected JSON object")
        return None
    return value


def read_text(path: Path, diagnostics: list[str]) -> str:
    """Read optional text and append a diagnostic when it is missing."""

    if not path.is_file():
        diagnostics.append(f"{path}: missing comparison Markdown")
        return ""
    return path.read_text(encoding="utf-8")


def validate_terlan_report_keys(payload: dict[str, Any], diagnostics: list[str]) -> None:
    """Validate that every VM lane artifact is represented in the report."""

    reports = payload.get("terlan_vm_lane_reports")
    if not isinstance(reports, dict):
        diagnostics.append(f"{COMPARISON_JSON}: terlan_vm_lane_reports must be an object")
        return
    missing = sorted(REQUIRED_TERLAN_REPORT_KEYS - set(reports))
    if missing:
        diagnostics.append(
            f"{COMPARISON_JSON}: missing Terlan VM lane report(s): {', '.join(missing)}"
        )


def validate_terlan_socket_report_metadata(
    payload: dict[str, Any], diagnostics: list[str]
) -> None:
    """Validate concurrency metadata recorded by every socket benchmark report."""

    reports = payload.get("terlan_vm_lane_reports")
    if not isinstance(reports, dict):
        return
    for key, report in sorted(reports.items()):
        if not isinstance(key, str) or not key.startswith("socket-"):
            continue
        if not isinstance(report, str) or not report:
            diagnostics.append(f"{COMPARISON_JSON}: socket report {key} path must be a string")
            continue
        report_path = WORKSPACE_ROOT / report
        report_payload = read_json_object(report_path, diagnostics)
        if report_payload is None:
            continue
        for field in SOCKET_REPORT_REQUIRED_INT_FIELDS:
            value = report_payload.get(field)
            if not is_int(value):
                diagnostics.append(
                    f"{report_path}: socket report {key} field `{field}` must be an integer"
                )
            elif value < 0:
                diagnostics.append(
                    f"{report_path}: socket report {key} field `{field}` must be non-negative"
                )
        for positive_field in (
            "concurrency",
            "effective_concurrency",
            "queue_capacity",
            "acceptor_count",
            "handler_worker_count",
            "connection_count",
        ):
            value = report_payload.get(positive_field)
            if is_int(value) and value <= 0:
                diagnostics.append(
                    f"{report_path}: socket report {key} field `{positive_field}` "
                    "must be positive"
                )
        if not isinstance(report_payload.get("request_mix"), str) or not report_payload.get(
            "request_mix"
        ):
            diagnostics.append(f"{report_path}: socket report {key} request_mix must be non-empty")
        queue_pressure = report_payload.get("queue_pressure")
        if not isinstance(queue_pressure, dict):
            diagnostics.append(
                f"{report_path}: socket report {key} queue_pressure must be an object"
            )
            continue
        for field in SOCKET_QUEUE_PRESSURE_REQUIRED_INT_FIELDS:
            value = queue_pressure.get(field)
            if not is_int(value):
                diagnostics.append(
                    f"{report_path}: socket report {key} queue_pressure.{field} "
                    "must be an integer"
                )
            elif value < 0:
                diagnostics.append(
                    f"{report_path}: socket report {key} queue_pressure.{field} "
                    "must be non-negative"
                )


def validate_cowboy_report_reference(payload: dict[str, Any], diagnostics: list[str]) -> None:
    """Validate that the OTP/Cowboy baseline artifact is recorded."""

    report = payload.get("cowboy_http_report")
    if not isinstance(report, str) or not report:
        diagnostics.append(f"{COMPARISON_JSON}: cowboy_http_report must be a non-empty string")
        return
    report_path = WORKSPACE_ROOT / report
    if not report_path.is_file():
        diagnostics.append(f"{COMPARISON_JSON}: missing Cowboy report {report}")


def lanes_by_group(
    payload: dict[str, Any], diagnostics: list[str]
) -> dict[str, dict[str, Any]]:
    """Index benchmark lanes by group name."""

    lanes = payload.get("lanes")
    if not isinstance(lanes, list):
        diagnostics.append(f"{COMPARISON_JSON}: lanes must be an array")
        return {}
    output: dict[str, dict[str, Any]] = {}
    for index, lane in enumerate(lanes):
        if not isinstance(lane, dict):
            diagnostics.append(f"{COMPARISON_JSON}: lane {index} must be an object")
            continue
        group = lane.get("group")
        if not isinstance(group, str):
            diagnostics.append(f"{COMPARISON_JSON}: lane {index} missing string group")
            continue
        output[group] = lane
    return output


def validate_group_evidence(
    lanes: dict[str, dict[str, Any]],
    required: dict[str, str],
    diagnostics: list[str],
) -> None:
    """Validate required lanes and their evidence classes."""

    for group, expected_evidence in required.items():
        lane = lanes.get(group)
        if lane is None:
            diagnostics.append(f"{COMPARISON_JSON}: missing lane group {group}")
            continue
        if lane.get("terlan_name") != TERLAN_LANE:
            diagnostics.append(f"{COMPARISON_JSON}: {group} must use Terlan lane {TERLAN_LANE}")
        if lane.get("evidence") != expected_evidence:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} evidence must be {expected_evidence}"
            )


def validate_cowboy_lanes(
    lanes: dict[str, dict[str, Any]], diagnostics: list[str]
) -> None:
    """Validate sustained Cowboy/OTP comparison lanes."""

    expected_names = {
        "socket-add-keepalive-c100-cowboy": (
            "cowboy_add_keep_alive_http_get_100_clients_request_time"
        ),
        "socket-add-keepalive-c1000-cowboy": (
            "cowboy_add_keep_alive_http_get_1000_clients_request_time"
        ),
    }
    for group, expected_name in expected_names.items():
        lane = lanes.get(group)
        if lane is None:
            continue
        if lane.get("competitor_name") != expected_name:
            diagnostics.append(f"{COMPARISON_JSON}: {group} competitor must be {expected_name}")
        if "Cowboy/Ranch on OTP" not in str(lane.get("note", "")):
            diagnostics.append(f"{COMPARISON_JSON}: {group} note must identify Cowboy/Ranch on OTP")


def validate_required_metrics(
    lanes: dict[str, dict[str, Any]], diagnostics: list[str]
) -> None:
    """Validate sustained add-handler rows have comparable metric fields."""

    for group in (
        "socket-add-keepalive-c100",
        "socket-add-keepalive-c1000",
        "socket-add-keepalive-c100-hyper",
        "socket-add-keepalive-c1000-hyper",
        "socket-add-keepalive-c100-cowboy",
        "socket-add-keepalive-c1000-cowboy",
    ):
        lane = lanes.get(group)
        if lane is None:
            continue
        for field in (
            "terlan_mean_us",
            "competitor_mean_us",
            "terlan_p99_us",
            "competitor_p99_us",
            "terlan_rps",
            "competitor_rps",
            "winner",
        ):
            if lane.get(field) in (None, "", "missing"):
                diagnostics.append(f"{COMPARISON_JSON}: {group} missing {field}")


def validate_http_realism_matrix(
    payload: dict[str, Any],
    lanes: dict[str, dict[str, Any]],
    diagnostics: list[str],
) -> None:
    """Validate benchmark realism classification for every comparable row."""

    matrix = payload.get("http_realism_matrix")
    if not isinstance(matrix, dict):
        diagnostics.append(f"{COMPARISON_JSON}: http_realism_matrix must be an object")
        return
    for group, lane in sorted(lanes.items()):
        if lane.get("terlan_name") != TERLAN_LANE:
            continue
        entry = matrix.get(group)
        if not isinstance(entry, dict):
            diagnostics.append(f"{COMPARISON_JSON}: missing realism matrix entry for {group}")
            continue
        classification = entry.get("classification")
        if classification not in {"advisory", "release-decisive"}:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} realism classification must be advisory "
                "or release-decisive"
            )
        dimensions = entry.get("dimensions")
        if not isinstance(dimensions, dict):
            diagnostics.append(f"{COMPARISON_JSON}: {group} realism dimensions must be an object")
            continue
        missing_dimensions = sorted(REQUIRED_REALISM_DIMENSIONS - set(dimensions))
        if missing_dimensions:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} realism matrix missing dimension(s): "
                f"{', '.join(missing_dimensions)}"
            )
        partial_or_missing = False
        for dimension in sorted(REQUIRED_REALISM_DIMENSIONS):
            status = dimensions.get(dimension)
            if status not in REALISM_STATUSES:
                diagnostics.append(
                    f"{COMPARISON_JSON}: {group} realism dimension {dimension} "
                    f"has invalid status {status!r}"
                )
            if status in {"partial", "missing"}:
                partial_or_missing = True
        if partial_or_missing and classification != "advisory":
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} must be advisory when any realism "
                "dimension is partial or missing"
            )
        if not isinstance(entry.get("reason"), str) or not entry.get("reason"):
            diagnostics.append(f"{COMPARISON_JSON}: {group} realism entry missing reason")


def validate_statistical_credibility_matrix(
    payload: dict[str, Any],
    lanes: dict[str, dict[str, Any]],
    diagnostics: list[str],
) -> None:
    """Validate sustained benchmark rows are marked directional until repeated."""

    matrix = payload.get("statistical_credibility_matrix")
    if not isinstance(matrix, dict):
        diagnostics.append(
            f"{COMPARISON_JSON}: statistical_credibility_matrix must be an object"
        )
        return
    for group, lane in sorted(lanes.items()):
        if lane.get("terlan_name") != TERLAN_LANE:
            continue
        entry = matrix.get(group)
        if not isinstance(entry, dict):
            diagnostics.append(
                f"{COMPARISON_JSON}: missing statistical credibility entry for {group}"
            )
            continue
        classification = entry.get("classification")
        if classification not in STATISTICAL_CREDIBILITY_CLASSES:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} statistical credibility class "
                f"must be one of {', '.join(sorted(STATISTICAL_CREDIBILITY_CLASSES))}"
            )
        sample_count = entry.get("sample_count")
        required_sample_count = entry.get("required_sample_count")
        if not is_int(sample_count):
            diagnostics.append(f"{COMPARISON_JSON}: {group} sample_count must be an integer")
            continue
        if not is_int(required_sample_count):
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} required_sample_count must be an integer"
            )
            continue
        if required_sample_count <= 0:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} required_sample_count must be positive"
            )
        required_metrics = entry.get("required_metrics")
        if not isinstance(required_metrics, list) or not all(
            isinstance(metric, str) for metric in required_metrics
        ):
            diagnostics.append(f"{COMPARISON_JSON}: {group} required_metrics must be strings")
        elif set(required_metrics) != STATISTICAL_REQUIRED_METRICS:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} required_metrics must be "
                f"{', '.join(sorted(STATISTICAL_REQUIRED_METRICS))}"
            )
        if lane.get("evidence") == "sustained-keepalive":
            if required_sample_count < SUSTAINED_REQUIRED_SAMPLE_COUNT:
                diagnostics.append(
                    f"{COMPARISON_JSON}: {group} sustained rows require at least "
                    f"{SUSTAINED_REQUIRED_SAMPLE_COUNT} samples"
                )
            if sample_count < required_sample_count and classification != "directional-single-run":
                diagnostics.append(
                    f"{COMPARISON_JSON}: {group} must be directional-single-run "
                    "until repeated samples exist"
                )
            if sample_count < SUSTAINED_REQUIRED_SAMPLE_COUNT:
                diagnostics.append(
                    f"{COMPARISON_JSON}: {group} has {sample_count} sustained sample(s); "
                    f"{SUSTAINED_REQUIRED_SAMPLE_COUNT} are required for the baseline"
                )
            if sample_count >= required_sample_count and classification != "statistically-credible":
                diagnostics.append(
                    f"{COMPARISON_JSON}: {group} must be statistically-credible "
                    "when repeated samples exist"
                )
        elif classification != "not-required":
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} non-sustained rows must be not-required"
            )
        if not isinstance(entry.get("reason"), str) or not entry.get("reason"):
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} statistical credibility entry missing reason"
            )


def validate_repeated_run_summary(
    payload: dict[str, Any],
    lanes: dict[str, dict[str, Any]],
    diagnostics: list[str],
) -> None:
    """Validate repeated-run min/median/max summaries."""

    requested = payload.get("comparison_runs_requested")
    completed = payload.get("comparison_runs_completed")
    validate_comparison_sample_counts(requested, completed, diagnostics)
    summary = payload.get("repeated_run_summary")
    if not isinstance(summary, dict):
        diagnostics.append(f"{COMPARISON_JSON}: repeated_run_summary must be an object")
        return
    for group, lane in sorted(lanes.items()):
        if lane.get("terlan_name") != TERLAN_LANE:
            continue
        entry = summary.get(group)
        if not isinstance(entry, dict):
            diagnostics.append(f"{COMPARISON_JSON}: missing repeated-run summary for {group}")
            continue
        complete_count = entry.get("complete_sample_count")
        if not is_int(complete_count):
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} complete_sample_count must be an integer"
            )
        elif lane.get("sample_count") != complete_count:
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} lane sample_count must match "
                "repeated-run complete_sample_count"
            )
        for field in (
            "terlan_mean_us",
            "competitor_mean_us",
            "terlan_throughput_requests_per_second",
            "competitor_throughput_requests_per_second",
        ):
            validate_numeric_summary(group, field, entry.get(field), diagnostics)


def validate_comparison_sample_counts(
    requested: Any,
    completed: Any,
    diagnostics: list[str],
) -> None:
    """Validate the comparison baseline has enough repeated runs."""

    if not is_int(requested) or requested < 1:
        diagnostics.append(
            f"{COMPARISON_JSON}: comparison_runs_requested must be a positive integer"
        )
    elif requested < SUSTAINED_REQUIRED_SAMPLE_COUNT:
        diagnostics.append(
            f"{COMPARISON_JSON}: comparison_runs_requested must be at least "
            f"{SUSTAINED_REQUIRED_SAMPLE_COUNT} for the HTTP baseline"
        )
    if not is_int(completed) or completed < 1:
        diagnostics.append(
            f"{COMPARISON_JSON}: comparison_runs_completed must be a positive integer"
        )
    elif completed < SUSTAINED_REQUIRED_SAMPLE_COUNT:
        diagnostics.append(
            f"{COMPARISON_JSON}: comparison_runs_completed must be at least "
            f"{SUSTAINED_REQUIRED_SAMPLE_COUNT} for the HTTP baseline"
        )


def validate_numeric_summary(
    group: str,
    field: str,
    value: Any,
    diagnostics: list[str],
) -> None:
    """Validate one min/median/max summary object."""

    if not isinstance(value, dict):
        diagnostics.append(f"{COMPARISON_JSON}: {group} {field} must be an object")
        return
    for key in ("min", "median", "max"):
        metric = value.get(key)
        if metric is not None and not is_int(metric):
            diagnostics.append(
                f"{COMPARISON_JSON}: {group} {field}.{key} must be an integer or null"
            )
    minimum = value.get("min")
    median = value.get("median")
    maximum = value.get("max")
    if all(is_int(metric) for metric in (minimum, median, maximum)) and not (
        minimum <= median <= maximum
    ):
        diagnostics.append(
            f"{COMPARISON_JSON}: {group} {field} must satisfy min <= median <= max"
        )


def validate_performance_clue_reports(
    payload: dict[str, Any],
    lanes: dict[str, dict[str, Any]],
    diagnostics: list[str],
) -> None:
    """Validate code-level clues for every comparable VM performance issue."""

    reports = payload.get("performance_clue_reports")
    if not isinstance(reports, list) or not reports:
        diagnostics.append(f"{COMPARISON_JSON}: performance_clue_reports must be a non-empty array")
        return
    report_texts: list[str] = []
    for report in reports:
        if not isinstance(report, str) or not report:
            diagnostics.append(f"{COMPARISON_JSON}: performance clue report path must be a string")
            continue
        report_path = WORKSPACE_ROOT / report
        if not report_path.is_file():
            diagnostics.append(f"{COMPARISON_JSON}: missing performance clue report {report}")
            continue
        report_texts.append(report_path.read_text(encoding="utf-8"))
    combined = "\n".join(report_texts)
    for fragment in (
        "Suspected VM subsystem",
        "Source files/functions",
        "Measured symptom",
        "Next optimization hypothesis",
    ):
        if fragment not in combined:
            diagnostics.append(
                f"{COMPARISON_JSON}: performance clue report missing `{fragment}`"
            )
    throughput_contradiction_found = False
    for group, lane in sorted(lanes.items()):
        if lane.get("terlan_name") != TERLAN_LANE:
            continue
        if not requires_performance_clue(lane):
            continue
        if has_throughput_contradiction(lane):
            throughput_contradiction_found = True
        if group not in combined:
            diagnostics.append(
                f"{COMPARISON_JSON}: performance clue report must mention performance "
                f"investigation lane {group}"
            )
    if throughput_contradiction_found and "throughput loses" not in combined:
        diagnostics.append(
            f"{COMPARISON_JSON}: performance clue report must describe throughput losses"
        )


def requires_performance_clue(lane: dict[str, Any]) -> bool:
    """Return whether one comparison row needs source-level performance clues."""

    if lane.get("terlan_mean_us") is None or lane.get("competitor_mean_us") is None:
        return False
    winner = lane.get("winner")
    if winner not in ("terlan-vm", "missing", None):
        return True
    return has_throughput_contradiction(lane)


def has_throughput_contradiction(lane: dict[str, Any]) -> bool:
    """Return whether Terlan wins latency but loses sustained throughput."""

    return (
        lane.get("evidence") == "sustained-keepalive"
        and lane.get("winner") == "terlan-vm"
        and is_int(lane.get("terlan_rps"))
        and is_int(lane.get("competitor_rps"))
        and lane["competitor_rps"] > lane["terlan_rps"]
    )


def is_int(value: Any) -> bool:
    """Return whether a JSON value is an integer metric."""

    return isinstance(value, int) and not isinstance(value, bool)


def validate_markdown(markdown: str, diagnostics: list[str]) -> None:
    """Validate the human-readable report keeps evidence visible."""

    if not markdown:
        return
    required_fragments = (
        "| Group | Terlan lane |",
        "Evidence",
        "socket-add-keepalive-c100",
        "socket-add-keepalive-c100-cowboy",
        "socket-add-keepalive-c1000-cowboy",
        "sustained-keepalive",
        "advisory-one-request-churn",
        "HTTP Realism Matrix",
        "Repeated Run Summary",
        "Complete samples",
        "Statistical Credibility Matrix",
        "statistically-credible",
        "Performance Clue Reports",
    )
    for fragment in required_fragments:
        if fragment not in markdown:
            diagnostics.append(f"{COMPARISON_MD}: missing fragment `{fragment}`")


def run_self_test() -> int:
    """Exercise credibility checks without requiring live benchmark artifacts."""

    diagnostics: list[str] = []
    validate_comparison_sample_counts(1, 1, diagnostics)
    require_any_diagnostic(
        diagnostics,
        "comparison_runs_requested must be at least 3",
        "one-run requested baseline rejection",
    )
    require_any_diagnostic(
        diagnostics,
        "comparison_runs_completed must be at least 3",
        "one-run completed baseline rejection",
    )

    diagnostics = []
    validate_comparison_sample_counts(3, 3, diagnostics)
    require_no_diagnostics(diagnostics, "three-run baseline acceptance")

    sustained_lane = {
        "terlan_name": TERLAN_LANE,
        "evidence": "sustained-keepalive",
    }
    required_metrics = sorted(STATISTICAL_REQUIRED_METRICS)
    diagnostics = []
    validate_statistical_credibility_matrix(
        {
            "statistical_credibility_matrix": {
                "socket-add-keepalive-c100": {
                    "classification": "directional-single-run",
                    "sample_count": 1,
                    "required_sample_count": SUSTAINED_REQUIRED_SAMPLE_COUNT,
                    "required_metrics": required_metrics,
                    "reason": "directional",
                }
            }
        },
        {"socket-add-keepalive-c100": sustained_lane},
        diagnostics,
    )
    require_any_diagnostic(
        diagnostics,
        "3 are required for the baseline",
        "one-run sustained lane rejection",
    )

    diagnostics = []
    validate_statistical_credibility_matrix(
        {
            "statistical_credibility_matrix": {
                "socket-add-keepalive-c100": {
                    "classification": "statistically-credible",
                    "sample_count": SUSTAINED_REQUIRED_SAMPLE_COUNT,
                    "required_sample_count": SUSTAINED_REQUIRED_SAMPLE_COUNT,
                    "required_metrics": required_metrics,
                    "reason": "credible",
                }
            }
        },
        {"socket-add-keepalive-c100": sustained_lane},
        diagnostics,
    )
    require_no_diagnostics(diagnostics, "three-run sustained lane acceptance")
    print("[vm-http-concurrency-investigation-check] self-test passed")
    return 0


def require_any_diagnostic(
    diagnostics: list[str],
    fragment: str,
    label: str,
) -> None:
    """Fail self-test if an expected diagnostic fragment is absent."""

    if not any(fragment in diagnostic for diagnostic in diagnostics):
        raise AssertionError(f"{label}: missing diagnostic `{fragment}`")


def require_no_diagnostics(diagnostics: list[str], label: str) -> None:
    """Fail self-test if diagnostics were unexpectedly produced."""

    if diagnostics:
        raise AssertionError(f"{label}: unexpected diagnostics: {diagnostics}")


if __name__ == "__main__":
    raise SystemExit(main())
