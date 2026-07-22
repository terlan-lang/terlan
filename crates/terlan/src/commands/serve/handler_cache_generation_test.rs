//! Full-cycle evidence for hot-reloaded native handler generation lifetime.

use std::fs;
use std::sync::{Arc, Weak};

use crate::runtime::vm::ReplValue;
use crate::support::test_fs;

use super::*;

const MODULE: &str = "app.ReloadGeneration";

/// Renders one source generation with a distinguishable native result.
fn source(value: i64) -> String {
    format!("module {MODULE}.\n\npub value(): Int -> {value}.\n")
}

/// Executes the generation marker through its admitted native image.
fn execute(runtime: &AotHandlerRuntime) -> i64 {
    let mut output = |_line: &str| {};
    let value = runtime
        .execute_immediate_native(MODULE, "value", Vec::new(), &mut output)
        .expect("execute generation marker");
    let ReplValue::Int(value) = value else {
        panic!("generation marker returned {value:?}")
    };
    value
}

/// Requires a retired generation to become unreachable after its final lease.
fn assert_retired(generation: &Weak<AotHandlerRuntime>) {
    assert!(
        generation.upgrade().is_none(),
        "retired native generation retained an undisclosed owner"
    );
}

/// Proves cache replacement isolates in-flight code and unloads at quiescence.
#[test]
fn hot_reload_pins_in_flight_generation_until_its_last_lease_drops() {
    let root = test_fs::temp_path("serve", "aot_handler_generation_lifetime");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/ReloadGeneration.terl");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::create_dir_all(&web_root).expect("create native output directory");

    fs::write(&source_path, source(7)).expect("write first generation");
    let first =
        cached_source_entry(&web_root, &source_path, MODULE).expect("compile first generation");
    let first_runtime = Arc::clone(&first.runtime);
    let first_retirement = Arc::downgrade(&first_runtime);
    assert_eq!(execute(&first_runtime), 7);

    fs::write(&source_path, source(700)).expect("write replacement generation");
    let second = cached_source_entry(&web_root, &source_path, MODULE)
        .expect("compile replacement generation");
    let second_runtime = Arc::clone(&second.runtime);
    let second_retirement = Arc::downgrade(&second_runtime);
    assert!(!Arc::ptr_eq(&first_runtime, &second_runtime));
    assert_eq!(execute(&first_runtime), 7);
    assert_eq!(execute(&second_runtime), 700);

    drop(first);
    drop(first_runtime);
    assert_retired(&first_retirement);

    cache()
        .expect("handler cache")
        .remove(&source_path)
        .expect("remove current generation");
    drop(second);
    drop(second_runtime);
    assert_retired(&second_retirement);
    fs::remove_dir_all(root).expect("cleanup generation fixture");
}

/// Proves HTTP compilation removes serialized and renamed runtime artifacts.
#[test]
fn handler_cache_compilation_removes_legacy_runtime_sidecars() {
    let root = test_fs::temp_path("serve", "aot_handler_legacy_artifacts");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/ReloadGeneration.terl");
    let vm_dir = web_root.join(".terlan/serve-aot/vm");
    let renamed_json = vm_dir.join("renamed.tvm");
    let json_sidecar = vm_dir.join("app_ReloadGeneration.tvm.json");
    let reuse_sidecar = vm_dir.join("app_ReloadGeneration.tvm.reuse");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::create_dir_all(&vm_dir).expect("create seeded native output directory");
    fs::write(&source_path, source(19)).expect("write native handler source");
    fs::write(
        &renamed_json,
        br#"{"vm_ir":{"functions":[{"instructions":["interpret-me"]}]}}"#,
    )
    .expect("write renamed serialized body");
    fs::write(&json_sidecar, b"serialized instructions").expect("write JSON sidecar");
    fs::write(&reuse_sidecar, b"legacy reuse marker").expect("write reuse sidecar");

    let entry = cached_source_entry(&web_root, &source_path, MODULE)
        .expect("compile and admit native handler");

    assert_eq!(execute(&entry.runtime), 19);
    assert!(!renamed_json.exists());
    assert!(!json_sidecar.exists());
    assert!(!reuse_sidecar.exists());
    assert_eq!(
        fs::read_dir(&vm_dir)
            .expect("read native output directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|value| value == "tvm"))
            .count(),
        1,
        "HTTP compilation must publish one native image"
    );

    cache()
        .expect("handler cache")
        .remove(&source_path)
        .expect("remove admitted handler");
    drop(entry);
    fs::remove_dir_all(root).expect("cleanup legacy HTTP fixture");
}
