//! Report consumers need local artifacts even when source tests are covered.

use super::*;

const REPORTS: [(&str, &str); 5] = [
    ("runtime::vm::memory::memory_test::limits_and_accounting::memory_accounting_writes_deterministic_pressure_report", "vm-memory-pressure-report.json"),
    ("runtime::vm::memory::memory_test::shared_ownership_and_soak::memory_accounting_soak_preserves_ownership_and_writes_benchmark_report", "vm-memory-soak-report.json"),
    ("runtime::vm::scheduler::scheduler_test::scheduler_writes_fairness_report_with_starvation_evidence", "vm-scheduler-fairness-report.json"),
    ("runtime::vm::timer::timer_test::cancellation_and_events::timer_table_writes_deadline_report_from_runtime_events", "vm-timer-deadline-report.json"),
    ("runtime::native_boundary::worker::worker_test::worker_lifecycle_events_write_native_boundary_report", "vm-native-boundary-report.json"),
];

#[test]
fn fresh_runtime_consumers_produce_reports_once_and_propagate_failure() {
    let fixture = fixture();
    let root = &fixture.0;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repository.join("Makefile")).unwrap();
    let mut makefile = String::from(
        "POLICY_CARGO_TEST = bash scripts/run_exact_cargo_test.sh\n\
         EXACT_CARGO_TEST = true\nRUST_TEST = true\n\
         vm-process-model-check vm-resource-ownership-check vm-scheduler-contract-check lean-proof-track-check:\n\t@:\n",
    );
    for target in [
        "vm-runtime-report-artifacts:",
        "vm-memory-heap-pressure-check:",
        "vm-scheduler-fairness-check:",
        "vm-timer-deadline-check:",
        "vm-native-boundary-contract-check:",
    ] {
        makefile.push_str(make_block(&source, target, "\n\n"));
        makefile.push('\n');
    }
    write(root, "Makefile", makefile);
    write(
        root,
        "scripts/run_exact_cargo_test.sh",
        fs::read(repository.join("scripts/run_exact_cargo_test.sh")).unwrap(),
    );
    let arms = REPORTS
        .iter()
        .map(|(name, file)| format!("{name:?} => {file:?},"))
        .collect::<String>();
    let record = format!(
        r#"pub fn record(name: &str) {{
            assert!(std::env::var_os("TERLAN_FIXTURE_REPORT_FAILURE").is_none());
            let file = match name {{ {arms} _ => panic!("unexpected test") }};
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../target/quality");
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join(file);
            // A duplicate producer must fail, including under parallel Make.
            use std::io::Write;
            let mut output = std::fs::OpenOptions::new().write(true).create_new(true)
                .open(path).unwrap();
            output.write_all(b"{{\"fixture\":true}}\n").unwrap();
        }}"#
    );
    let names = REPORTS
        .iter()
        .map(|(name, _)| (*name).into())
        .collect::<Vec<_>>();
    write(
        root,
        "terlan/src/lib.rs",
        format!(
            "{record}{}",
            test_module(&names, "").replace("#[ignore]", "")
        ),
    );
    assert!(run(command(root, "cargo").arg("generate-lockfile")));
    for failure in [false, true] {
        let mut make = command(root, "make");
        make.args([
            "--no-print-directory",
            "-j4",
            "vm-memory-heap-pressure-check",
            "vm-scheduler-fairness-check",
            "vm-timer-deadline-check",
            "vm-native-boundary-contract-check",
        ]);
        if failure {
            make.env("TERLAN_FIXTURE_REPORT_FAILURE", "1");
        }
        assert_eq!(run(&mut make), !failure);
        for (_, file) in REPORTS {
            let path = root.join("target/quality").join(file);
            assert_eq!(path.exists(), !failure, "{file}");
            if !failure {
                assert_eq!(fs::read(&path).unwrap(), b"{\"fixture\":true}\n");
                fs::remove_file(path).unwrap();
            }
        }
    }
}
