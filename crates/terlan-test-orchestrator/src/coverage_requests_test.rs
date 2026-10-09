use super::*;
use serde_json::json;

fn selection(args: &[&str]) -> Result<Selection, PhaseFailure> {
    parse(
        &args
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
    )
}

#[test]
fn library_and_integration_requests_preserve_shared_exact_and_skip_semantics() {
    let selected = selection(&[
        "--locked",
        "-p",
        "terlan",
        "--lib",
        "--features",
        "quality-tools,editor-lsp",
        "some_test",
        "--",
        "--exact",
    ])
    .unwrap();
    assert_eq!(selected.package, "terlan");
    assert_eq!(selected.kind, "lib");
    assert_eq!(selected.selectors, ["some_test", "--exact"]);
    let selected = selection(&[
        "-p",
        "terlan",
        "--test",
        "direct_aot_local_shard",
        "--",
        "--exact",
        "native_image_transitions_execute_on_the_local_shard",
    ])
    .unwrap();
    assert_eq!(selected.kind, "test");
    assert_eq!(selected.target.as_deref(), Some("direct_aot_local_shard"));
    assert!(selection(&[
        "--package",
        "terlan-process-owner",
        "--lib",
        "--",
        "--skip",
        "external"
    ])
    .is_ok());
    assert!(selection(&["-p", "terlan", "--bin", "terlc"]).is_ok());
}

#[test]
fn presentation_flags_preserve_the_requested_test_selection() {
    let inventory = ["module_layout_a", "module_layout_ab", "unrelated"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    for (target, presentation) in [vec!["--lib"], vec!["--test", "layout"]]
        .into_iter()
        .flat_map(|target| ["--nocapture", "--quiet", "-q"].map(|flag| (target.clone(), flag)))
    {
        let mut arguments = vec!["-p", "terlan"];
        arguments.extend(target);
        arguments.extend(["module_layout_a", "--", presentation, "--exact"]);
        let selected = selection(&arguments).unwrap();
        let selectors = selected
            .selectors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            crate::test_inventory::select(&inventory, &selectors).unwrap(),
            vec![&"module_layout_a".to_owned()]
        );
        arguments.extend(["--skip", "module_layout_a"]);
        let selected = selection(&arguments).unwrap();
        let selectors = selected
            .selectors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert!(crate::test_inventory::select(&inventory, &selectors)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn unsupported_build_policies_cannot_be_silently_covered_by_debug_tests() {
    assert!(selection(&["-p", "terlan", "--lib", "--", "--test-threads=1"]).is_err());
    for option in [
        "--release",
        "--profile",
        "--target",
        "--manifest-path",
        "--no-default-features",
        "--all-features",
        "--config",
        "--workspace",
        "--no-run",
    ] {
        assert!(
            selection(&["-p", "terlan", "--lib", option]).is_err(),
            "{option}"
        );
    }
    for args in [
        vec!["--lib"],
        vec!["-p", "terlan"],
        vec!["-p", "terlan", "--test"],
        vec!["-p", "terlan", "--lib", "--bin", "terlc"],
        vec!["-p", "terlan", "--lib", "first", "second"],
        vec!["-p", "terlan", "--lib", "--features", "unknown"],
        vec!["-p", "another", "--lib", "--features", "quality-tools"],
        vec!["-p", "terlan*", "--lib"],
        vec!["-p", "terlan", "-p", "terlan", "--lib"],
        vec!["-p", "terlan", "--lib", "--", "--list"],
    ] {
        assert!(selection(&args).is_err(), "{args:?}");
    }
}

#[test]
fn batch_identity_is_unique_but_multiple_gates_may_request_the_same_completed_test() {
    let arguments = json!(["-p", "terlan", "--lib", "shared_test"]);
    let batch = json!([{"id":"first","arguments":arguments},{"id":"second","arguments":arguments}]);
    assert_eq!(
        parse_batch(&serde_json::to_vec(&batch).unwrap())
            .unwrap()
            .len(),
        2
    );
    for invalid in [
        json!([]),
        json!([{"id":"first","arguments":arguments},{"id":"first","arguments":arguments}]),
        json!([{"id":"first","arguments":arguments,"ignored":true}]),
        json!([{"id":"","arguments":arguments}]),
    ] {
        assert!(parse_batch(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    assert!(parse_batch(&vec![b' '; 1024 * 1024 + 1]).is_err());
}
