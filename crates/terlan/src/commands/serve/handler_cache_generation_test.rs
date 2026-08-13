//! Full-cycle evidence for hot-reloaded native handler generation lifetime.

use std::fs;
use std::sync::{Arc, Mutex, Weak};

use crate::runtime::vm::ReplValue;
use crate::support::test_fs;

use super::*;

const MODULE: &str = "app.ReloadGeneration";
static GENERATION_TEST_LOCK: Mutex<()> = Mutex::new(());

fn generation_test_guard() -> std::sync::MutexGuard<'static, ()> {
    GENERATION_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Renders one source generation whose marker crosses a deep native tail loop.
fn source(value: i64) -> String {
    format!(
        "module {MODULE}.\n\n\
         loop(N: Int, Acc: Int): Int ->\n\
             if {{ N == 0 -> Acc; true -> loop(N - 1, Acc) }}.\n\n\
         pub value(): Int -> loop(1000000, {value}).\n"
    )
}

fn source_with_state(value: i64) -> String {
    format!(
        "module {MODULE}.\n\n\
         pub struct State {{\n    value: Int\n}}.\n\n\
         pub value(): Int -> {value}.\n"
    )
}

fn write_reload_manifest(web_root: &Path) {
    fs::write(
        web_root.join("manifest.json"),
        format!(
            r#"{{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [{{
    "method": "GET",
    "route": "/value",
    "module": "{MODULE}",
    "function": "value",
    "arity": 0,
    "source": {{"path": "src/app/ReloadGeneration.terl", "line": 1, "column": 1}}
  }}],
  "assets": []
}}"#
        ),
    )
    .expect("write reload manifest");
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

/// Proves replacement isolates deep tail-loop code and unloads at quiescence.
#[test]
fn hot_reload_pins_in_flight_generation_until_its_last_lease_drops() {
    let _guard = generation_test_guard();
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
    invalidate_vm_handler_cache();
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

    // The handler cache is process-global and other parallel serve tests may
    // invalidate it. Invalidation is the production retirement operation and
    // is idempotent, unlike asserting that this test still owns the map slot.
    invalidate_vm_handler_cache();
    drop(second);
    drop(second_runtime);
    assert_retired(&second_retirement);
    fs::remove_dir_all(root).expect("cleanup generation fixture");
}

/// Proves the watcher transaction keeps serving the last valid native image.
#[test]
fn developer_reload_is_atomic_compatible_and_failed_edit_safe() {
    let _guard = generation_test_guard();
    let root = test_fs::temp_path("serve", "aot_developer_reload_transaction");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/ReloadGeneration.terl");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::create_dir_all(&web_root).expect("create web root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"reload-test\"\nversion = \"0.0.1\"\n",
    )
    .expect("write project manifest");
    write_reload_manifest(&web_root);
    fs::write(&source_path, source(11)).expect("write first generation");

    let first = cached_source_entry(&web_root, &source_path, MODULE).expect("admit first");
    let pinned = Arc::clone(&first.runtime);
    fs::write(&source_path, source(22)).expect("write compatible edit");
    source_generation::developer_reload::compile_and_publish_source_batch(
        &web_root,
        std::slice::from_ref(&source_path),
    )
    .expect("admit compatible edit");
    let second = cached_source_entry(&web_root, &source_path, MODULE).expect("resolve second");
    assert_eq!(
        execute(&pinned),
        11,
        "in-flight call keeps its native image"
    );
    assert_eq!(
        execute(&second.runtime),
        22,
        "new calls use replacement image"
    );

    fs::write(
        &source_path,
        "module app.ReloadGeneration.\n\npub value( ->\n",
    )
    .expect("write broken edit");
    let broken = source_generation::developer_reload::compile_and_publish_source_batch(
        &web_root,
        std::slice::from_ref(&source_path),
    )
    .expect_err("broken edit must fail before publication");
    source_generation::developer_reload::record_failed_reload(&web_root, &broken)
        .expect("record failed continuity");
    assert_eq!(execute(&second.runtime), 22);
    assert_eq!(
        execute(
            &cached_source_entry(&web_root, &source_path, MODULE)
                .expect("old cache remains active")
                .runtime
        ),
        22
    );

    fs::write(&source_path, source_with_state(33)).expect("write incompatible state edit");
    let incompatible = source_generation::developer_reload::compile_and_publish_source_batch(
        &web_root,
        std::slice::from_ref(&source_path),
    )
    .expect_err("state shape edit must require restart");
    assert!(
        incompatible.contains("incompatible_state"),
        "{incompatible}"
    );
    assert_eq!(execute(&second.runtime), 22, "state survives rejection");

    fs::write(&source_path, source(44)).expect("write corrected edit");
    source_generation::developer_reload::compile_and_publish_source_batch(
        &web_root,
        std::slice::from_ref(&source_path),
    )
    .expect("corrected edit activates");
    let corrected =
        cached_source_entry(&web_root, &source_path, MODULE).expect("resolve corrected generation");
    assert_eq!(execute(&corrected.runtime), 44);

    let active_path = web_root.join(".terlan/serve-aot/runtime/active.json");
    let active = fs::read(&active_path).expect("read active pointer");
    let mut stale: serde_json::Value =
        serde_json::from_slice(&active).expect("decode active pointer");
    stale["modules"][MODULE] = serde_json::Value::String("generations/missing.json".into());
    fs::write(
        &active_path,
        serde_json::to_vec(&stale).expect("encode stale pointer"),
    )
    .expect("write stale active pointer");
    let partial = source_generation::activate_persisted_generation(&web_root)
        .expect_err("partial generation must fail closed");
    assert!(partial.contains("partial_generation"), "{partial}");
    assert_eq!(execute(&corrected.runtime), 44);
    fs::write(&active_path, active).expect("restore active pointer");

    let report =
        fs::read_to_string(web_root.join(".terlan/serve-aot/watch-mode-hot-reload-report.json"))
            .expect("read reload report");
    assert!(report.contains("failed_build_continuity"));
    assert!(report.contains("direct_aot"));
    assert!(!report.contains("interpreter"));
    invalidate_vm_handler_cache();
    drop(first);
    drop(pinned);
    drop(second);
    drop(corrected);
    fs::remove_dir_all(root).expect("cleanup reload transaction fixture");
}

/// Proves HTTP compilation removes serialized and renamed runtime artifacts.
#[test]
fn handler_cache_compilation_removes_legacy_runtime_sidecars() {
    let _guard = generation_test_guard();
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

    // Other serve tests share the process-wide cache and may invalidate it
    // concurrently. Use the production retirement operation instead of
    // assuming this test still owns the map slot; panicking while holding the
    // write guard would poison every subsequent cache user in the suite.
    invalidate_vm_handler_cache();
    drop(entry);
    fs::remove_dir_all(root).expect("cleanup legacy HTTP fixture");
}

/// Persisted generation metadata must fail closed when its native image changes.
#[test]
fn persisted_generation_rejects_tampered_native_image() {
    let _guard = generation_test_guard();
    let root = test_fs::temp_path("serve", "aot_handler_generation_integrity");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/ReloadGeneration.terl");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::create_dir_all(&web_root).expect("create native output directory");
    fs::write(&source_path, source(29)).expect("write handler source");

    let entry =
        cached_source_entry(&web_root, &source_path, MODULE).expect("compile persisted generation");
    assert_eq!(execute(&entry.runtime), 29);
    drop(entry);
    invalidate_vm_handler_cache();

    let generation_path = source_generation::active_generation_metadata_path(&web_root, MODULE)
        .expect("resolve active metadata")
        .expect("active module metadata");
    let generation: serde_json::Value =
        serde_json::from_slice(&fs::read(&generation_path).expect("read persisted generation"))
            .expect("decode persisted generation");
    let image = generation["image"]
        .as_str()
        .map(std::path::PathBuf::from)
        .expect("persisted image path");
    let mut bytes = fs::read(&image).expect("read persisted native image");
    bytes.push(0);
    fs::write(&image, bytes).expect("tamper persisted native image");

    let error = match cached_source_entry(&web_root, &source_path, MODULE) {
        Ok(_) => panic!("tampered image must not be admitted"),
        Err(error) => error,
    };
    assert!(error.contains("image integrity mismatch"), "{error}");
    fs::remove_dir_all(root).expect("cleanup integrity fixture");
}

#[test]
fn immediate_callback_executes_on_its_protocol_owner_without_rpc() {
    let _guard = generation_test_guard();
    let root = test_fs::temp_path("serve", "aot_handler_owner_local");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/ReloadGeneration.terl");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::create_dir_all(&web_root).expect("create native output directory");
    fs::write(&source_path, source(23)).expect("write handler source");
    let entry = cached_source_entry(&web_root, &source_path, MODULE)
        .expect("compile owner-local generation");

    let value = crate::runtime::vm::protocol_task_executor::with_protocol_scheduler_for_test(
        VmSchedulerId::primary(),
        || execute(&entry.runtime),
    );

    assert_eq!(value, 23);
    assert!(
        entry.runtime.generation.shards[0].initialized().is_none(),
        "owner-local immediate HTTP must not start the asynchronous shard thread"
    );
    let metrics = entry.runtime.generation.shards[0].telemetry_snapshot();
    assert_eq!(metrics.entries, 0);
    assert_eq!(metrics.completions, 0);
    cache()
        .expect("handler cache")
        .write()
        .expect("handler cache write")
        .remove(&source_path);
    drop(entry);
    fs::remove_dir_all(root).expect("cleanup owner-local fixture");
}

#[test]
fn admitted_body_handler_retains_compiler_request_projection() {
    let _guard = generation_test_guard();
    const REQUEST_MODULE: &str = "app.RequestProjection";
    let root = test_fs::temp_path("serve", "aot_handler_request_projection");
    let web_root = root.join("_build/web");
    let source_path = root.join("src/app/RequestProjection.terl");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::create_dir_all(&web_root).expect("create native output directory");
    fs::write(
        &source_path,
        "module app.RequestProjection.\n\nimport std.http.Response.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(request: Request): Response ->\n    Response.text(request.body_text()).\n",
    )
    .expect("write request handler");

    let entry = cached_source_entry(&web_root, &source_path, REQUEST_MODULE)
        .expect("compile projected handler");
    assert_eq!(
        entry
            .runtime
            .request_projection(REQUEST_MODULE, "handle", 1),
        crate::runtime::native::http::RequestFieldProjection::Fields(
            1 << crate::runtime::native::http::RequestFieldProjection::BODY,
        )
    );
    assert_eq!(
        entry
            .runtime
            .scalar_request_ingress(REQUEST_MODULE, "handle", 1)
            .map(|(_, field)| field),
        Some(crate::runtime::native::http::RequestFieldProjection::BODY)
    );
    let projection = crate::runtime::native::http::RequestFieldProjection::Fields(
        1 << crate::runtime::native::http::RequestFieldProjection::BODY,
    );
    let request = crate::terlan_native::http::Request::new("typed response body").into_parts();
    let response = crate::runtime::vm::protocol_task_executor::with_protocol_scheduler_for_test(
        VmSchedulerId::primary(),
        || {
            entry
                .runtime
                .execute_projected_http_request(
                    REQUEST_MODULE,
                    "handle",
                    request,
                    projection,
                    &mut |_| {},
                )
                .expect("execute typed HTTP response")
        },
    );
    let crate::runtime::vm::VmHttpCallResult::Response(response) = response else {
        panic!("direct handler did not use the typed managed Response projection")
    };
    assert_eq!(response.kind, 0);
    assert_eq!(response.status, 200);
    assert_eq!(response.payload, "typed response body");
    assert!(response.headers.is_empty());

    let large_body = "x".repeat(4 * 1024);
    let request = crate::terlan_native::http::Request::new(large_body.clone()).into_parts();
    let response = crate::runtime::vm::protocol_task_executor::with_protocol_scheduler_for_test(
        VmSchedulerId::primary(),
        || {
            entry
                .runtime
                .execute_projected_http_request(
                    REQUEST_MODULE,
                    "handle",
                    request,
                    projection,
                    &mut |_| {},
                )
                .expect("execute transferred typed HTTP response")
        },
    );
    let crate::runtime::vm::VmHttpCallResult::Response(response) = response else {
        panic!("large direct handler did not use the typed Response projection")
    };
    assert_eq!(&response.payload[..], large_body.as_bytes());

    cache()
        .expect("handler cache")
        .write()
        .expect("handler cache write")
        .remove(&source_path);
    drop(entry);
    fs::remove_dir_all(root).expect("cleanup request projection fixture");
}
