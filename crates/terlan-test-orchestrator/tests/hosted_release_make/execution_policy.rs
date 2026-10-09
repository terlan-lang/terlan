//! Publication must execute requests whose policy differs from hosted coverage.
use super::*;

#[test]
fn explicit_execution_policies_run_locally_and_propagate_failure() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repository.join("Makefile")).unwrap();
    let assignment = source
        .lines()
        .find(|line| line.starts_with("POLICY_CARGO_TEST = $(TERLAN_RUST_ORCHESTRATOR)"))
        .unwrap();
    for (target, count) in [
        ("lalrpop-parser-parity-check", 1),
        ("tvm-aot-compilation-time-check", 2),
    ] {
        let production = rule(&source, target);
        let prerequisites = production
            .lines()
            .next()
            .unwrap()
            .split_once(':')
            .unwrap()
            .1;
        for fail in [false, true] {
            let fixture = fixture();
            fs::create_dir_all(fixture.0.join("scripts")).unwrap();
            fs::copy(
                repository.join("scripts/run_exact_cargo_test.sh"),
                fixture.0.join("scripts/run_exact_cargo_test.sh"),
            )
            .unwrap();
            let worker = fixture.0.join("worker");
            fs::write(
                &worker,
                r#"#!/bin/sh
set -eu
test "$1" = --run-owned
test "$2" = --timeout-seconds
test "$3" = 30
test "$4" = --
shift 4
exec "$@"
"#,
            )
            .unwrap();
            let cargo = fixture.0.join("cargo");
            fs::write(
                &cargo,
                r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> invoked
test "$FAIL_PRODUCER" = 0
echo 'running 1 test'
"#,
            )
            .unwrap();
            fs::set_permissions(cargo, fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(fixture.0.join("Makefile"), format!(
                "TERLAN_RUST_ORCHESTRATOR := ./worker\nTERLAN_COMPILER_BUILD_TIMEOUT_SECONDS := 30\nRUST_TEST := true\nEXACT_CARGO_TEST := true\n{assignment}\n{production}\n{prerequisites}:\n"
            )).unwrap();
            let mut command = Command::new("make");
            command
                .current_dir(&fixture.0)
                .args(["--no-print-directory", target])
                .env(
                    "PATH",
                    format!("{}:{}", fixture.0.display(), std::env::var("PATH").unwrap()),
                )
                .env("FAIL_PRODUCER", if fail { "1" } else { "0" })
                .env_remove("MAKEFLAGS")
                .env_remove("MAKEOVERRIDES")
                .env_remove("MFLAGS");
            let result = ProcessControl::new(Duration::from_secs(10)).run(&mut command, |_| Ok(()));
            assert_eq!(result.is_ok(), !fail, "target={target}, fail={fail}");
            let calls = fs::read_to_string(fixture.0.join("invoked")).unwrap();
            assert_eq!(calls.lines().count(), if fail { 1 } else { count });
            for call in calls.lines() {
                if target == "lalrpop-parser-parity-check" {
                    assert!(call.ends_with("compiler::syntax:: -- --test-threads=1"));
                } else {
                    assert!(call.contains("--release -p terlan --test direct_aot_cache"));
                    assert!(call.ends_with("-- --exact"));
                }
            }
        }
    }
}
