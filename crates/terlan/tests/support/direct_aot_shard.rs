//! Application-level assertions executed by the production shard scheduler.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use terlan_process_owner::ProcessControl;

#[path = "aot_failure.rs"]
mod failure;

// Each expression is a separate entrypoint so one actor's mailbox, exit, or
// scheduling state cannot influence a later assertion. All entries share the
// same compiled image; this does not introduce another compiler invocation.
const CASES: &[&str] = &[
    "add(1, 2) == 3",
    "subtract(7, 3) == 4",
    "multiply(6, 7) == 42",
    "divide(42, 6) == 7",
    "remainder(43, 6) == 1",
    "negate(7) == -7",
    "add_twice(40, 1) == 42",
    "classify(6) == 13",
    "choose(false) == 0",
    "choose(true) == 1",
    "bool_not(false)",
    "not bool_not(true)",
    "bool_and(true, true)",
    "not bool_and(true, false)",
    "bool_or(false, true)",
    "not short_and()",
    "short_or()",
    "float_add(1.5, 2.25) == 3.75",
    "float_mixed(1, 2.75) == 3.75",
    "float_compare(3.75, 2)",
    "yielded_add(41) == 42",
    "yielded_local_from(21) == 43",
    "yielded_add_twice(41) == 43",
    "yielded_bool_twice(true)",
    "branch_yield(false) == 7",
    "branch_yield(true) == 41",
    "branch_yield_local(true, 40) == 42",
    "branch_yield_local(false, 40) == 40",
    "branch_yield_both(true) == 1",
    "branch_yield_both(false) == 2",
    "nested_branch_yield(true, true) == 11",
    "nested_branch_yield(true, false) == 12",
    "nested_branch_yield(false, true) == 13",
    "not short_circuit_yield(false, true)",
    "short_circuit_yield(true, true)",
    "yield_then_branch(true) == 21",
    "yield_then_branch(false) == 22",
    "branch_capture_pair(true, 20, 22) == 42",
    "branch_yield_only(true) == 1",
    "send_capture_to_self(41) == 42",
    "send_call_capture_to_self(41) == 42",
    "Process.send_int(1, 1); receive_capture(41) == 42",
    "Process.send_int(1, 1); receive_call_capture(41) == 42",
    "spawn_capture(Process.entry[Int](4), 41) == 42",
    "spawn_call_capture(Process.entry[Int](4), 41) == 42",
    "timer_capture(41) == 42",
    "timer_call_capture(41) == 42",
    "resource_capture(Process.resource_kind[Int](7), 41) == 42",
    "resource_call_capture(Process.resource_kind[Int](7), 41) == 42",
    "map_identity(Map.new[String, Int]()).is_empty()",
    "set_identity(Set.new[String]()).is_empty()",
    "list_identity([1, 2]) == [1, 2]",
    "atom_identity(Ready) == Ready",
];

const ENTRY_ROOTS: &[&str] = &[
    "yielded_local",
    "managed_constructed",
    "managed_yielded",
    "spawn_managed_child",
    "managed_returned",
    "spawn_failure_isolated",
    "spawn_failed_child",
    "schedule_normal_then_true",
];

const FAILURES: &[(&str, &[&str])] = &[
    ("spawn_invalid_entry", &["pure_native_spawn_entry"]),
    ("timer_zero", &["Timer delay must be positive"]),
    ("timer_negative", &["Timer delay must be positive"]),
    (
        "resource_zero_invalid",
        &["Resource kind tag must be positive"],
    ),
    ("fail_self", &["pure_native_failure", "native_failure:"]),
    ("failure_zero_invalid", &["Failure code must be positive"]),
];

/// Adds callable checks to the existing compiler fixture before its sole build.
pub(super) fn source() -> String {
    // Executable builds retain only the main closure. Anchor these callable
    // test roots just as the shared fixture anchors its transition scenarios;
    // the false branch is never executed by the ordinary main-entry smoke.
    let mut source = include_str!("../fixtures/direct_aot.terl").replace(
        "false -> (local_shard_transition_roots(); managed_continuation_roots());",
        "false -> (shard_contract_roots(); local_shard_transition_roots(); managed_continuation_roots());",
    );
    source = source.replace(
        "module direct_aot.",
        "module direct_aot.\nimport std.collections.{Map, Set}.",
    );
    for (index, expression) in CASES.iter().enumerate() {
        writeln!(
            source,
            "\npub shard_case_{index}(): Bool ->\n    {expression}."
        )
        .expect("append shard assertion");
    }
    source.push_str("\npub shard_contract_roots(): Bool ->\n");
    for index in 0..CASES.len() {
        writeln!(source, "    shard_case_{index}();").expect("anchor shard assertion");
    }
    for entry in ENTRY_ROOTS
        .iter()
        .copied()
        .chain(FAILURES.iter().map(|(entry, _)| *entry))
    {
        writeln!(source, "    {entry}();").expect("anchor VM lifecycle assertion");
    }
    source.push_str("    true.\n");
    source
}

/// Runs every selected check, retaining all failures instead of stopping early.
pub(super) fn assert_execution(image: &Path) {
    let mut failures = Vec::new();
    for (index, expression) in CASES.iter().enumerate() {
        let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-vm"));
        command
            .arg("run")
            .arg(image)
            .arg("--entry")
            .arg(format!("shard_case_{index}"))
            .arg("--test-eval");
        if let Err(error) =
            ProcessControl::new(Duration::from_secs(30)).run(&mut command, |_| Ok(()))
        {
            failures.push(format!("{expression}: {error:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "shard assertions failed:\n{}",
        failures.join("\n")
    );
    for entry in [
        "spawn_then_send",
        "spawn_failure_isolated",
        "timer_then_true",
        "resource_then_true",
        "spawn_failed_child",
        "schedule_priority_then_true",
        "schedule_normal_then_true",
        "schedule_background_then_true",
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-vm"));
        command
            .arg("run")
            .arg(image)
            .args(["--entry", entry, "--test-eval"]);
        ProcessControl::new(Duration::from_secs(30))
            .run(&mut command, |_| Ok(()))
            .unwrap_or_else(|error| panic!("{entry}: {error:?}"));
    }
    for (entry, diagnostics) in FAILURES {
        failure::assert_vm_failure(image, entry, diagnostics);
    }
}
