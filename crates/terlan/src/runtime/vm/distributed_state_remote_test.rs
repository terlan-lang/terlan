use super::{evaluate_call, is_store, write};
use crate::runtime::vm::ReplValue;

fn call(function: &str, args: &[ReplValue]) -> ReplValue {
    evaluate_call("std.vm.DistributedState", function, args)
        .expect("distributed state call should dispatch")
        .expect("distributed state call should succeed")
}

#[test]
fn adapter_preserves_write_conflict_and_checkpoint_semantics() {
    let scope = call(
        "scope",
        &[
            ReplValue::String("settings".to_string()),
            ReplValue::String("theme".to_string()),
        ],
    );
    let policy = call(
        "policy",
        &[ReplValue::String("explicit_user_resolution".to_string())],
    );
    let mut store = call("store", &[]);
    assert!(is_store(&store));

    let first = write(
        &mut store,
        &[
            scope.clone(),
            ReplValue::String("node-a".to_string()),
            ReplValue::String("dark".to_string()),
            call(
                "version",
                &[ReplValue::Int(5), ReplValue::String("node-a".to_string())],
            ),
            policy.clone(),
        ],
    )
    .expect("first write should apply");
    assert_eq!(
        call("kind", &[first]),
        ReplValue::String("applied".to_string())
    );

    let stale = write(
        &mut store,
        &[
            scope.clone(),
            ReplValue::String("node-b".to_string()),
            ReplValue::String("light".to_string()),
            call(
                "version",
                &[ReplValue::Int(4), ReplValue::String("node-b".to_string())],
            ),
            policy,
        ],
    )
    .expect("stale write should return an outcome");
    assert_eq!(
        call("kind", &[stale.clone()]),
        ReplValue::String("conflict".to_string())
    );
    assert!(matches!(
        call("conflict", &[stale]),
        ReplValue::Tuple(items)
            if matches!(items.first(), Some(ReplValue::Atom(tag)) if tag == "vm_distributed_state_conflict")
    ));

    let snapshot = evaluate_call("__receiver__", "export_snapshot", &[store])
        .expect("snapshot receiver call should dispatch")
        .expect("snapshot export should succeed");
    let restored = call("restore", &[snapshot]);
    let entry = evaluate_call("__receiver__", "get", &[restored, scope])
        .expect("get receiver call should dispatch")
        .expect("restored entry should exist");
    assert!(matches!(
        entry,
        ReplValue::Tuple(items)
            if matches!(items.first(), Some(ReplValue::Atom(tag)) if tag == "vm_distributed_state_entry")
    ));
}

#[test]
fn adapter_rejects_invalid_policy_and_missing_scope() {
    let error = evaluate_call(
        "std.vm.DistributedState",
        "policy",
        &[ReplValue::String("unknown".to_string())],
    )
    .expect("policy call should dispatch")
    .expect_err("unknown policy should fail");
    assert!(error.contains("unsupported conflict policy"));

    let store = call("store", &[]);
    let scope = call(
        "scope",
        &[
            ReplValue::String("missing".to_string()),
            ReplValue::String("key".to_string()),
        ],
    );
    let error = evaluate_call("__receiver__", "get", &[store, scope])
        .expect("get call should dispatch")
        .expect_err("missing scope should fail");
    assert!(error.contains("state scope is not present"));
}
