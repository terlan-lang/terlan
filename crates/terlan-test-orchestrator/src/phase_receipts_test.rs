use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use crate::{PhaseExecutor, TestShard, ValidationTier};
use std::collections::BTreeSet;
use std::time::Duration;

fn phase() -> TestPhase {
    TestPhase {
        name: "receipt fixture",
        tier: ValidationTier::FastUnit,
        executor: PhaseExecutor::TerlanHarness,
        args: vec!["fixture::"],
        environment: vec![("FIXTURE", "one".into())],
        shard: Some(TestShard { index: 0, total: 2 }),
        test_threads: Some(2),
    }
}

fn expectation(name: &str) -> ExpectedTests {
    ExpectedTests {
        passed: BTreeSet::from([name.to_owned()]),
        ignored: BTreeSet::new(),
        filtered: 3,
    }
}

fn evidence(expected: &ExpectedTests) -> TestEvidence {
    TestEvidence(serde_json::json!({
        "scope":"admitted-libtest-records-v1",
        "selection_identity_sha256":expected.identity(),
        "passed":1, "ignored":0, "filtered":3,
        "result_log_bytes":42,
        "result_log_identity_sha256":"a".repeat(64),
    }))
}

fn store(root: PathBuf) -> Store {
    Store {
        root: Some(root),
        input_identity: "b".repeat(64),
        force: false,
        pending: BTreeMap::new(),
    }
}

#[test]
fn exact_receipt_is_published_immediately_and_reused() {
    let fixture = temporary_fixture("phase-receipt");
    let root = fixture.0.join("receipts");
    let phase = phase();
    let expected = expectation("fixture::passes");
    let control = ProcessControl::new(Duration::from_secs(5));
    let mut store = store(root);

    store
        .remember(&phase, 2, &expected, &evidence(&expected))
        .unwrap();
    assert!(store
        .restore(&phase, 2, &expected, control)
        .unwrap()
        .is_some());
    assert!(store
        .restore(&phase, 2, &expectation("fixture::changed"), control)
        .unwrap()
        .is_none());
}

#[test]
fn malformed_exact_receipt_fails_closed() {
    let fixture = temporary_fixture("phase-receipt-corrupt");
    let root = fixture.0.join("receipts");
    std::fs::create_dir(&root).unwrap();
    let store = store(root.clone());
    let phase = phase();
    let expected = expectation("fixture::passes");
    let key = store.key(&phase, 2, &expected).unwrap();
    std::fs::write(root.join(format!("{key}.json")), b"{}").unwrap();

    assert!(store
        .restore(
            &phase,
            2,
            &expected,
            ProcessControl::new(Duration::from_secs(5)),
        )
        .is_err());
}

#[test]
fn force_mode_executes_instead_of_reusing() {
    let fixture = temporary_fixture("phase-receipt-force");
    let root = fixture.0.join("receipts");
    let phase = phase();
    let expected = expectation("fixture::passes");
    let control = ProcessControl::new(Duration::from_secs(5));
    let mut store = store(root);
    store
        .remember(&phase, 2, &expected, &evidence(&expected))
        .unwrap();
    store.force = true;
    assert!(store
        .restore(&phase, 2, &expected, control)
        .unwrap()
        .is_none());
}

#[test]
fn receipt_keys_ignore_routes_but_bind_semantic_environment() {
    let fixture = temporary_fixture("phase-receipt-routing");
    let store = store(fixture.0.join("receipts"));
    let expected = expectation("fixture::passes");
    let mut first = phase();
    first.environment.extend([
        ("TERLAN_TEST_COMPILER", "/first/terlc".into()),
        ("TERLAN_TEST_VM", "/first/vm".into()),
        ("TERLAN_TEST_CAPABILITY_WORKER", "/first/worker".into()),
    ]);
    let mut second = first.clone();
    for (_, value) in &mut second.environment[1..] {
        *value = "/second/route".into();
    }
    assert_eq!(
        store.key(&first, 2, &expected).unwrap(),
        store.key(&second, 2, &expected).unwrap()
    );
    second.environment[0].1 = "changed".into();
    assert_ne!(
        store.key(&first, 2, &expected).unwrap(),
        store.key(&second, 2, &expected).unwrap()
    );
}
