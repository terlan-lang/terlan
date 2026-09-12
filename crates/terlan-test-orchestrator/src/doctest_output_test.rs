use super::*;

fn summary(passed: usize, ignored: usize) -> String {
    format!("test result: ok. {passed} passed; 0 failed; {ignored} ignored; 0 measured; 0 filtered out; finished in 0.01s\n")
}

fn complete() -> String {
    format!(
        "\nrunning 3 tests\ntest source with spaces.rs - sample (line 2) ... ok\ntest lib.rs - compile (line 12) - compile fail ... ok\ntest lib.rs - later (line 30) ... ignored\n\n{}\n",
        summary(2, 1)
    )
}

#[test]
fn emitted_harnesses_reconcile_names_modes_counts_and_empty_packages() {
    let output = format!(
        "{}all doctests ran in 0.20s; merged doctests compilation took 0.10s\n\nrunning 0 tests\n{}{}",
        complete(), summary(0, 0), complete()
    );
    let evidence = inspect(output.as_bytes()).unwrap().json();
    assert_eq!(evidence["scope"], "rustdoc-emitted-harness-records-v1");
    assert_eq!(evidence["independent_inventory"], false);
    assert_eq!(evidence["passed"], 4);
    assert_eq!(evidence["ignored"], 2);
    assert_eq!(evidence["harnesses"].as_array().unwrap().len(), 3);
    assert_eq!(evidence["harnesses"][0], evidence["harnesses"][2]);
    assert_ne!(evidence["harnesses"][0], evidence["harnesses"][1]);
    let empty = format!("\nrunning 0 tests\n{}", summary(0, 0));
    assert_eq!(inspect(empty.as_bytes()).unwrap().json()["passed"], 0);
}

#[test]
fn doctest_summaries_cannot_hide_missing_duplicated_or_failed_records() {
    let good = complete();
    for bad in [
        String::new(),
        summary(2, 1),
        good.replace("running 3 tests", "running 4 tests"),
        good.replace("2 passed", "3 passed"),
        good.replace("0 failed", "1 failed"),
        good.replace("0 filtered out", "1 filtered out"),
        good.replace("0 measured", "1 measured"),
        good.replace(" ... ignored", " ... ok"),
        good.replace(" ... ok", " ... FAILED"),
        good.replace("test lib.rs - later (line 30) ... ignored\n", ""),
        good.replace(
            "test lib.rs - later (line 30) ... ignored",
            "test source with spaces.rs - sample (line 2) ... ok",
        ),
        good.replace("running 3 tests", "running 3 tests\nrunning 0 tests"),
        good.replace("running 3 tests", "running 100001 tests"),
        good.replace("running 3 tests", "running 3 test"),
        good.replace("0.01s", "NaNs"),
        good.replace("0.01s", "-1s"),
        good.replace("0.01s", "infs"),
        good.replace("0.01s", "0.01s; additional"),
        format!("{good}test unexpected ... ok\n"),
        format!("{good}test result: FAILED.\n"),
        format!("{good}running 1 test\ntest unfinished ... "),
        format!("{good}unexplained output\n"),
        good.trim_end().to_owned(),
    ] {
        assert!(inspect(bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(inspect(b"\xff\n").is_err());
}

#[test]
fn doctest_observations_are_bounded_and_hash_name_and_classification() {
    let empty = format!("running 0 tests\n{}", summary(0, 0));
    assert!(inspect(empty.repeat(MAX_HARNESSES).as_bytes()).is_ok());
    assert!(inspect(empty.repeat(MAX_HARNESSES + 1).as_bytes()).is_err());
    let good = complete();
    let changed = good.replace("sample (line 2)", "sample (line 3)");
    assert_ne!(
        inspect(good.as_bytes()).unwrap().json()["harnesses"][0]["records_identity_sha256"],
        inspect(changed.as_bytes()).unwrap().json()["harnesses"][0]["records_identity_sha256"]
    );
}

#[test]
fn slow_serial_and_parallel_doctests_still_require_a_terminal_result() {
    let name = "lib.rs - example (line 4)";
    for output in [
        format!("running 1 test\ntest {name} ... test {name} has been running for over 60 seconds\nok\n{}", summary(1, 0)),
        format!("running 1 test\ntest {name} - compile ... test {name} has been running for over 60 seconds\nok\n{}", summary(1, 0)),
        format!("running 1 test\ntest {name} has been running for over 60 seconds\ntest {name} ... ok\n{}", summary(1, 0)),
    ] {
        assert_eq!(inspect(output.as_bytes()).unwrap().json()["passed"], 1);
        assert!(inspect(output.replace("over 60", "over 0").as_bytes()).is_err());
        assert!(inspect(output.replace("has been", "WRONG has been").as_bytes()).is_err());
        assert!(inspect(output.replace("ok\n", "FAILED\n").as_bytes()).is_err());
    }
    let missing = format!(
        "running 1 test\ntest {name} ... test {name} has been running for over 60 seconds\n{}",
        summary(1, 0)
    );
    assert!(inspect(missing.as_bytes()).is_err());
}
