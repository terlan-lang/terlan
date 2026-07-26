#!/usr/bin/env python3
"""Run and compare deterministic Terlan binary/transport workloads.

The Rust benchmark owns executable VM measurements. This driver owns the
versioned workload manifest, real socket-concurrency sweep, baseline
comparison, JSON/TSV publication, and stable winner/delta diagnostics.
"""

from __future__ import annotations

import argparse
import csv
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
WORKSPACE = ROOT.parent
MANIFEST = Path(__file__).with_name("workloads.json")
RAW_REPORT = ROOT / "target/quality/vm-binary-protocol-benchmark.json"
CURRENT_JSON = ROOT / "target/quality/protocol-stack-comparison.json"
CURRENT_TSV = ROOT / "target/quality/protocol-stack-comparison.tsv"
REPORT_DIR = WORKSPACE / "docs/benchmark_reports/protocol"
BASELINE_JSON = REPORT_DIR / "protocol-stack-baseline.v1.json"
BASELINE_TSV = REPORT_DIR / "protocol-stack-baseline.v1.tsv"
LEGACY_HTTP = WORKSPACE / "benchmarks/results/http-vm-vs-rust.latest.json"
SCALES = (1, 10, 100, 1_000)
PHASES = ("cold", "warm")
ROW_COLUMNS = (
    "runtime_lane",
    "profile",
    "commit",
    "platform",
    "rust_version",
    "workload",
    "workload_class",
    "phase",
    "iterations",
    "concurrency",
    "mean_us",
    "median_us",
    "p95_us",
    "p99_us",
    "error_rate_percent",
    "typed_decode_failure_count",
    "baseline_mean_us",
    "winner",
    "delta_percent",
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", action="store_true", help="execute all workloads")
    parser.add_argument("--validate-only", action="store_true")
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--anchor", choices=(
        "binary_protocol_benchmark",
        "binary_protocol_concurrency_benchmark",
    ))
    args = parser.parse_args(argv)
    manifest = read_json(MANIFEST)
    validate_manifest(manifest)

    if args.run:
        run_raw_benchmark()
        raw = read_json(RAW_REPORT)
        if raw.get("deterministic_seed") != manifest["deterministic_seed"]:
            raise RuntimeError("raw report deterministic seed differs from workload manifest")
        rows = normalize_raw_report(raw, metadata(raw))
        baseline_rows = [] if args.update_baseline else read_baseline_rows()
        compared = compare_rows(rows, baseline_rows, blessing=args.update_baseline)
        legacy = legacy_comparisons(manifest)
        report = build_report(raw, compared, legacy)
        persist_report(CURRENT_JSON, CURRENT_TSV, report, compared)
        if args.update_baseline:
            persist_report(BASELINE_JSON, BASELINE_TSV, report, compared)
        print_comparisons(compared, legacy)
    else:
        validate_baseline_artifacts(manifest)

    if args.anchor:
        validate_anchor(args.anchor, manifest)
        print(f"[{args.anchor}] passed")
    return 0


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"cannot read benchmark JSON `{path}`: {error}") from error
    if not isinstance(value, dict):
        raise RuntimeError(f"benchmark JSON `{path}` must contain an object")
    return value


def validate_manifest(manifest: dict[str, Any]) -> None:
    if manifest.get("schema") != "terlan.protocol-benchmark-workloads.v1":
        raise RuntimeError("protocol workload manifest schema changed")
    if manifest.get("scale_points") != list(SCALES):
        raise RuntimeError("protocol workloads must use scales 1, 10, 100, 1000")
    if manifest.get("warm_sample_count") != 3:
        raise RuntimeError("protocol workloads must retain three warm samples")
    if not isinstance(manifest.get("deterministic_seed"), int):
        raise RuntimeError("protocol workload deterministic seed is missing")
    declared = {
        cover
        for group in ("source_workloads", "transport_workloads", "http_workloads")
        for row in manifest.get(group, [])
        for cover in row.get("covers", [])
    }
    required = set(manifest.get("required_adversarial_coverage", []))
    if required != {
        "truncated-payloads",
        "invalid-utf8",
        "malformed-framing",
        "impossible-widths",
        "duplicate-captures",
        "unsupported-backend-paths",
    }:
        raise RuntimeError("protocol adversarial requirement inventory changed")
    missing = sorted(required - declared)
    if missing:
        raise RuntimeError(f"protocol adversarial coverage missing: {', '.join(missing)}")


def run_raw_benchmark() -> None:
    RAW_REPORT.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["TERLAN_BENCH_BINARY_PROTOCOL_OUTPUT"] = str(RAW_REPORT)
    run_checked(
        [
            "cargo",
            "run",
            "-p",
            "terlan",
            "--bin",
            "terlan-benchmark",
            "--quiet",
            "--",
            "vm-binary-protocol-baseline",
        ],
        env=env,
    )


def metadata(raw: dict[str, Any]) -> dict[str, Any]:
    return {
        "runtime_lane": raw.get("runtime_lane", "terlan-vm"),
        "profile": raw.get("profile", "test"),
        "commit": git_commit(),
        "platform": raw.get("platform", platform.platform()),
        "rust_version": raw.get("rustc_version") or "unknown",
    }


def git_commit() -> str:
    completed = subprocess.run(
        ["git", "rev-parse", "--short=12", "HEAD"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    return completed.stdout.strip() if completed.returncode == 0 else "unknown"


def normalize_raw_report(raw: dict[str, Any], meta: dict[str, Any]) -> list[dict[str, Any]]:
    if raw.get("schema") != "terlan.vm-binary-protocol-benchmark.v8":
        raise RuntimeError("binary protocol raw report schema must be v8")
    if raw.get("scale_points") != list(SCALES):
        raise RuntimeError("binary protocol raw report scale points changed")
    rows: list[dict[str, Any]] = []
    groups = (
        ("scenarios", "cold_end_to_end_us", "warm_end_to_end_samples_us"),
        ("transport_scenarios", "cold_measurement_us", "warm_measurement_samples_us"),
    )
    for group, cold_key, warm_key in groups:
        for scenario in raw.get(group, []):
            base = {
                **meta,
                "workload": scenario["id"],
                "workload_class": scenario["workload_class"],
                "iterations": scenario["operation_count"],
                "concurrency": scenario.get("concurrency", 1),
                "error_rate_percent": scenario["unexpected_error_rate_percent"],
                "typed_decode_failure_count": scenario.get("expected_typed_failure_count", 0),
            }
            rows.append(metric_row(base, "cold", [scenario[cold_key]]))
            rows.append(metric_row(base, "warm", scenario[warm_key]))
    validate_scale_coverage(rows)
    return rows


def metric_row(base: dict[str, Any], phase: str, values: list[int]) -> dict[str, Any]:
    if not values:
        raise RuntimeError(f"{base['workload']} {phase} has no measurements")
    ordered = sorted(int(value) for value in values)
    return {
        **base,
        "phase": phase,
        "mean_us": round(statistics.fmean(ordered), 3),
        "median_us": percentile(ordered, 50),
        "p95_us": percentile(ordered, 95),
        "p99_us": percentile(ordered, 99),
    }


def percentile(values: list[int], value: int) -> int:
    index = ((len(values) - 1) * value + 99) // 100
    return values[index]


def validate_scale_coverage(rows: list[dict[str, Any]]) -> None:
    by_workload: dict[str, set[tuple[int, str]]] = {}
    for row in rows:
        scale = int(str(row["workload"]).rsplit("-", 1)[-1])
        base = str(row["workload"]).rsplit("-", 1)[0]
        by_workload.setdefault(base, set()).add((scale, row["phase"]))
    expected = {(scale, phase) for scale in SCALES for phase in PHASES}
    missing = {name: expected - points for name, points in by_workload.items() if points != expected}
    if missing:
        raise RuntimeError(f"protocol workload scale/phase coverage changed: {missing}")


def row_key(row: dict[str, Any]) -> tuple[Any, ...]:
    return (row["workload"], row["phase"], row["iterations"], row["concurrency"])


def read_baseline_rows() -> list[dict[str, Any]]:
    report = read_json(BASELINE_JSON)
    rows = report.get("rows")
    if not isinstance(rows, list) or not rows:
        raise RuntimeError("protocol performance baseline has no rows")
    return rows


def compare_rows(
    rows: list[dict[str, Any]], baseline_rows: list[dict[str, Any]], *, blessing: bool
) -> list[dict[str, Any]]:
    baseline = {row_key(row): row for row in baseline_rows}
    compared: list[dict[str, Any]] = []
    for row in rows:
        reference = row if blessing else baseline.get(row_key(row))
        if reference is None:
            raise RuntimeError(f"protocol baseline is missing row {row_key(row)}")
        current = float(row["mean_us"])
        previous = float(reference["mean_us"])
        if previous == 0:
            delta = 0.0 if current == 0 else 100.0
        else:
            delta = round((current - previous) * 100.0 / previous, 3)
        winner = "tie"
        if delta < 0:
            winner = "current-terlan-vm"
        elif delta > 0:
            winner = "baseline-terlan-vm"
        compared.append(
            {
                **row,
                "baseline_mean_us": previous,
                "winner": winner,
                "delta_percent": delta,
            }
        )
    if len(compared) != len(baseline) and not blessing:
        raise RuntimeError("protocol baseline contains stale rows")
    return compared


def legacy_comparisons(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    rows = [dict(row) for row in manifest["legacy_lanes"]]
    cached = read_json(LEGACY_HTTP)
    by_group = {row.get("group"): row for row in cached.get("lanes", [])}
    comparable = rows[0]
    comparable["comparisons"] = []
    for scale in comparable["scales"]:
        lane = by_group.get(f"socket-crud-c{scale}")
        if lane is None or lane.get("delta_percent") is None:
            raise RuntimeError(f"cached Axum comparison is missing CRUD c{scale}")
        comparable["comparisons"].append(
            {
                "concurrency": scale,
                "winner": lane["winner"],
                "delta_percent": round(float(lane["delta_percent"]), 3),
                "source_group": lane["group"],
            }
        )
    return rows


def build_report(
    raw: dict[str, Any], rows: list[dict[str, Any]], legacy: list[dict[str, Any]]
) -> dict[str, Any]:
    classes = {row["workload_class"] for row in rows}
    if classes != {"success", "adversarial"}:
        raise RuntimeError("protocol comparison must contain success and adversarial classes")
    return {
        "schema": "terlan.protocol-stack-comparison.v1",
        "status": "completed",
        "runtime_lane": "terlan-vm",
        "profile": raw.get("profile", "test"),
        "commit": git_commit(),
        "platform": raw.get("platform", platform.platform()),
        "rust_version": raw.get("rustc_version") or "unknown",
        "deterministic_seed": raw.get("deterministic_seed"),
        "scale_points": list(SCALES),
        "phases": list(PHASES),
        "rows": rows,
        "legacy_lanes": legacy,
    }


def persist_report(
    json_path: Path,
    tsv_path: Path,
    report: dict[str, Any],
    rows: list[dict[str, Any]],
) -> None:
    json_path.parent.mkdir(parents=True, exist_ok=True)
    json_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    buffer = io.StringIO()
    writer = csv.DictWriter(buffer, fieldnames=ROW_COLUMNS, delimiter="\t", lineterminator="\n")
    writer.writeheader()
    for row in rows:
        writer.writerow({column: row[column] for column in ROW_COLUMNS})
    tsv_path.write_text(buffer.getvalue(), encoding="utf-8")


def print_comparisons(rows: list[dict[str, Any]], legacy: list[dict[str, Any]]) -> None:
    for row in rows:
        print(
            "[protocol-benchmark] "
            f"{row['workload']} phase={row['phase']} iterations={row['iterations']} "
            f"concurrency={row['concurrency']} winner={row['winner']} "
            f"delta={row['delta_percent']:+.3f}%"
        )
    for lane in legacy:
        if lane["status"] == "unsupported":
            print(
                f"[protocol-benchmark] legacy={lane['lane']} workload={lane['workload']} "
                f"winner=not-comparable delta=unsupported reason={lane['reason']}"
            )
        else:
            for comparison in lane["comparisons"]:
                print(
                    f"[protocol-benchmark] legacy={lane['lane']} workload={lane['workload']} "
                    f"concurrency={comparison['concurrency']} winner={comparison['winner']} "
                    f"delta={comparison['delta_percent']:.3f}%"
                )
    print(f"[protocol-benchmark] wrote {CURRENT_JSON}")
    print(f"[protocol-benchmark] wrote {CURRENT_TSV}")


def validate_baseline_artifacts(manifest: dict[str, Any]) -> None:
    report = read_json(BASELINE_JSON)
    if report.get("schema") != "terlan.protocol-stack-comparison.v1":
        raise RuntimeError("protocol baseline schema changed")
    if report.get("scale_points") != list(SCALES) or report.get("phases") != list(PHASES):
        raise RuntimeError("protocol baseline dimensions changed")
    rows = report.get("rows", [])
    if not rows:
        raise RuntimeError("protocol baseline rows are missing")
    for row in rows:
        missing = [column for column in ROW_COLUMNS if column not in row]
        if missing:
            raise RuntimeError(f"protocol baseline row missing fields: {', '.join(missing)}")
        if row["winner"] not in {"tie", "current-terlan-vm", "baseline-terlan-vm"}:
            raise RuntimeError("protocol baseline row has invalid winner")
        if not isinstance(row["delta_percent"], (int, float)):
            raise RuntimeError("protocol baseline row has no numeric delta")
    validate_scale_coverage(rows)
    tsv_lines = BASELINE_TSV.read_text(encoding="utf-8").splitlines()
    if not tsv_lines or tsv_lines[0].split("\t") != list(ROW_COLUMNS):
        raise RuntimeError("protocol baseline TSV header changed")
    if len(tsv_lines) != len(rows) + 1:
        raise RuntimeError("protocol JSON/TSV baseline row counts differ")
    legacy = report.get("legacy_lanes", [])
    if not any(row.get("status") == "comparable-cached" for row in legacy):
        raise RuntimeError("protocol baseline lacks a comparable legacy lane")
    if not any(row.get("status") == "unsupported" for row in legacy):
        raise RuntimeError("protocol baseline lacks unsupported comparison documentation")
    validate_manifest(manifest)


def validate_anchor(anchor: str, manifest: dict[str, Any]) -> None:
    validate_baseline_artifacts(manifest)
    rows = read_baseline_rows()
    if anchor == "binary_protocol_benchmark":
        classes = {row["workload_class"] for row in rows}
        if classes != {"success", "adversarial"}:
            raise RuntimeError("binary protocol anchor lacks success/adversarial rows")
    else:
        validate_aot_http_concurrency_artifact(read_json(LEGACY_HTTP))


def validate_aot_http_concurrency_artifact(report: dict[str, Any]) -> None:
    if report.get("benchmark") != "http-vm-vs-rust" or report.get("status") != "completed":
        raise RuntimeError("AOT HTTP concurrency comparison is not completed")
    lanes = {
        lane.get("group"): lane
        for lane in report.get("lanes", [])
        if isinstance(lane, dict)
    }
    required = {"socket-crud-c100", "socket-crud-c1000"}
    if not required.issubset(lanes):
        raise RuntimeError("AOT HTTP concurrency comparison lacks required CRUD scales")
    for group in sorted(required):
        lane = lanes[group]
        if lane.get("winner") not in {"terlan-vm", "axum-tokio", "tie"}:
            raise RuntimeError(f"AOT HTTP concurrency lane `{group}` has invalid winner")
        if not isinstance(lane.get("delta_percent"), (int, float)):
            raise RuntimeError(f"AOT HTTP concurrency lane `{group}` has no numeric delta")


def run_checked(command: list[str], *, env: dict[str, str] | None = None) -> None:
    completed = subprocess.run(command, cwd=ROOT, env=env, check=False)
    if completed.returncode != 0:
        raise RuntimeError(f"benchmark command failed ({completed.returncode}): {' '.join(command)}")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(f"[protocol-benchmark] error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
