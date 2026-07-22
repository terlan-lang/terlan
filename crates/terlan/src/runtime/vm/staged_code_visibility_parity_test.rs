use super::super::code_server::{VmCodeServer, VmCodeServerEvent, VmModuleGenerationState};
use super::super::process::{VmProcessSource, VmProcessTable};

const MODULE: &str = "code.call_fun_before_load";

fn source(body: &str) -> String {
    format!(
        "module {MODULE}.\n\n\
         pub run(value: Int): Int -> {body}.\n"
    )
}

/// Replaces OTP's before-load execution regression with explicit VM staging.
#[test]
fn compiled_module_remains_invisible_until_explicit_publication() {
    let mut code_server = VmCodeServer::default();
    let staged = VmCodeServer::stage_source("src/code/call_fun_before_load.terl", &source("value"))
        .expect("valid source should stage");

    assert!(!code_server.module_loaded(MODULE));
    assert!(!code_server.function_exported(MODULE, "run", 1));
    assert!(code_server.snapshots().is_empty());
    assert!(code_server.event_snapshots().is_empty());

    let event = code_server.publish_staged(staged);
    assert!(matches!(
        event,
        VmCodeServerEvent::Published { ref module, .. } if module == MODULE
    ));
    assert!(code_server.module_loaded(MODULE));
    assert!(code_server.function_exported(MODULE, "run", 1));
    assert_eq!(code_server.snapshots().len(), 1);
    assert_eq!(code_server.event_snapshots().len(), 1);
}

/// Proves staging replacement code cannot change active process ownership.
#[test]
fn staged_replacement_is_atomic_and_preserves_active_generation_bindings() {
    let mut processes = VmProcessTable::default();
    let existing_pid = processes.spawn_root(VmProcessSource::new(MODULE, "run", 1));
    let mut code_server = VmCodeServer::default();
    code_server
        .publish_source("src/code/call_fun_before_load.terl", &source("value + 1"))
        .expect("initial source should publish");
    let existing_binding = code_server
        .bind_process_to_active(&processes, existing_pid, MODULE)
        .expect("existing process should bind to active code");
    let snapshots_before = code_server.snapshots();
    let events_before = code_server.event_snapshots();

    let staged =
        VmCodeServer::stage_source("src/code/call_fun_before_load.terl", &source("value + 2"))
            .expect("replacement source should stage");

    assert_eq!(code_server.snapshots(), snapshots_before);
    assert_eq!(code_server.event_snapshots(), events_before);
    assert_eq!(
        code_server
            .active_generation(MODULE)
            .expect("active generation"),
        existing_binding.generation
    );

    let event = code_server.publish_staged(staged);
    let VmCodeServerEvent::HotReloaded {
        previous_generation,
        previous_state,
        active_generation,
        ..
    } = event
    else {
        panic!("replacement publication must hot reload");
    };
    assert_eq!(previous_generation, existing_binding.generation);
    assert_eq!(previous_state, VmModuleGenerationState::Retiring);
    assert_ne!(active_generation, existing_binding.generation);
    assert_eq!(
        code_server
            .snapshots_for_module(MODULE)
            .iter()
            .find(|snapshot| snapshot.generation == existing_binding.generation)
            .expect("bound generation must remain inspectable")
            .active_processes,
        1
    );
}
