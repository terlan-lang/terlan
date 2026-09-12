use super::*;
use std::ffi::OsString;

fn entries() -> Vec<(OsString, OsString)> {
    [
        ("GITHUB_ACTIONS", "true"),
        ("GITHUB_REPOSITORY", "terlan-lang/terlan"),
        ("GITHUB_SHA", "0123456789012345678901234567890123456789"),
        (
            "GITHUB_WORKFLOW_REF",
            "terlan-lang/terlan/.github/workflows/ci.yml@refs/heads/main",
        ),
        ("GITHUB_JOB", "check"),
        ("GITHUB_RUN_ID", "123456"),
        ("GITHUB_RUN_ATTEMPT", "2"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect()
}

fn environment(entries: Vec<(OsString, OsString)>) -> ExecutionEnvironment {
    ExecutionEnvironment::from_entries(entries, &std::env::current_dir().unwrap()).unwrap()
}

#[test]
fn local_execution_is_not_exported_as_an_authenticated_hosted_run() {
    assert_eq!(context(&environment(vec![])).unwrap(), Value::Null);
    let hosted = context(&environment(entries())).unwrap();
    assert_eq!(hosted["run_id"], 123456);
    assert_eq!(hosted["run_attempt"], 2);
    assert_eq!(hosted["job"], "check");
    assert_eq!(hosted["host_os"], std::env::consts::OS);
    assert_eq!(hosted["host_arch"], std::env::consts::ARCH);
    assert_eq!(hosted["authentication_required"], true);
}

#[test]
fn malformed_or_incomplete_hosted_coordinates_fail_closed() {
    for (key, value) in [
        ("GITHUB_REPOSITORY", "../terlan"),
        ("GITHUB_SHA", "not-a-revision"),
        (
            "GITHUB_WORKFLOW_REF",
            "elsewhere/other/.github/workflows/ci.yml@refs/heads/main",
        ),
        (
            "GITHUB_WORKFLOW_REF",
            "terlan-lang/terlan/.github/workflows/../../ci.yml@refs/heads/main",
        ),
        ("GITHUB_JOB", "check\nother"),
        ("GITHUB_RUN_ID", "0"),
        ("GITHUB_RUN_ATTEMPT", "02"),
        ("GITHUB_RUN_ATTEMPT", "18446744073709551616"),
    ] {
        let mut changed = entries();
        changed.iter_mut().find(|(name, _)| name == key).unwrap().1 = value.into();
        assert!(context(&environment(changed)).is_err(), "{key}");
    }
    for index in 1..entries().len() {
        let mut missing = entries();
        missing.remove(index);
        assert!(context(&environment(missing)).is_err());
    }
}
