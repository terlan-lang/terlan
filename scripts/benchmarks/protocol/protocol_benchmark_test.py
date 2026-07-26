"""Contract tests for the protocol benchmark publication driver."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("protocol_benchmark.py")
SPEC = importlib.util.spec_from_file_location("protocol_benchmark", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
protocol = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(protocol)


class ProtocolBenchmarkTest(unittest.TestCase):
    def test_binary_protocol_benchmark(self) -> None:
        manifest = protocol.read_json(protocol.MANIFEST)
        protocol.validate_manifest(manifest)
        covers = {
            cover
            for group in ("source_workloads", "transport_workloads", "http_workloads")
            for row in manifest[group]
            for cover in row["covers"]
        }
        self.assertIn("binary-construction", covers)
        self.assertIn("binary-decode-roundtrip", covers)
        self.assertIn("protocol-shape-recompose", covers)
        self.assertEqual(
            set(manifest["required_adversarial_coverage"]),
            {
                "truncated-payloads",
                "invalid-utf8",
                "malformed-framing",
                "impossible-widths",
                "duplicate-captures",
                "unsupported-backend-paths",
            },
        )

    def test_binary_protocol_concurrency_benchmark(self) -> None:
        report = {
            "benchmark": "http-vm-vs-rust",
            "status": "completed",
            "lanes": [
                {
                    "group": "socket-crud-c100",
                    "winner": "terlan-vm",
                    "delta_percent": 1.0,
                },
                {
                    "group": "socket-crud-c1000",
                    "winner": "axum-tokio",
                    "delta_percent": -1.0,
                },
            ],
        }
        protocol.validate_aot_http_concurrency_artifact(report)

    def test_comparison_records_numeric_winner_delta_for_adversarial_rows(self) -> None:
        row = {
            "runtime_lane": "terlan-vm",
            "profile": "test",
            "commit": "abc",
            "platform": "test-platform",
            "rust_version": "rustc test",
            "workload": "invalid-width-10",
            "workload_class": "adversarial",
            "phase": "warm",
            "iterations": 10,
            "concurrency": 1,
            "mean_us": 80,
            "median_us": 80,
            "p95_us": 90,
            "p99_us": 90,
            "error_rate_percent": 0.0,
            "typed_decode_failure_count": 10,
        }
        baseline = {**row, "mean_us": 100}
        compared = protocol.compare_rows([row], [baseline], blessing=False)
        self.assertEqual(compared[0]["winner"], "current-terlan-vm")
        self.assertEqual(compared[0]["delta_percent"], -20.0)

    def test_json_and_tsv_persistence_use_the_same_rows(self) -> None:
        row = {column: 0 for column in protocol.ROW_COLUMNS}
        row.update(
            runtime_lane="terlan-vm",
            profile="test",
            commit="abc",
            platform="test-platform",
            rust_version="rustc test",
            workload="fixed-header-1",
            workload_class="success",
            phase="cold",
            winner="tie",
        )
        with tempfile.TemporaryDirectory() as directory:
            json_path = Path(directory) / "report.json"
            tsv_path = Path(directory) / "report.tsv"
            protocol.persist_report(json_path, tsv_path, {"rows": [row]}, [row])
            self.assertIn('"rows"', json_path.read_text(encoding="utf-8"))
            self.assertEqual(len(tsv_path.read_text(encoding="utf-8").splitlines()), 2)


if __name__ == "__main__":
    unittest.main()
