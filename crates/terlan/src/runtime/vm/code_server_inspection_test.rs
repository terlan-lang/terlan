use super::{
    VmCodeServer, VmCodeServerEvent, VmModuleArtifact, VmModuleFunction,
    VmModuleGenerationState,
};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};

fn artifact(checksum: &str) -> VmModuleArtifact {
    VmModuleArtifact::new(checksum, format!("source-map-{checksum}"))
}

fn process_source(name: &str) -> VmProcessSource {
    VmProcessSource::new("module_info_test", name, 0)
}

#[test]
fn code_server_module_scoped_inspection_excludes_unrelated_lifecycle_traffic() {
    let mut code_server = VmCodeServer::default();
    let app_published = code_server.publish("app.Main", artifact("app-1"));
    code_server.publish("other.Main", artifact("other-1"));
    let app_reloaded = code_server.publish("app.Main", artifact("app-2"));

    let generations = code_server.snapshots_for_module("app.Main");
    assert_eq!(generations.len(), 2);
    assert!(generations
        .iter()
        .all(|snapshot| snapshot.module == "app.Main"));
    assert_eq!(generations[0].state, VmModuleGenerationState::Retired);
    assert_eq!(generations[0].checksum, "app-1");
    assert_eq!(generations[1].state, VmModuleGenerationState::Active);
    assert_eq!(generations[1].checksum, "app-2");

    let events = code_server.event_snapshots_for_module("app.Main");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].event, app_published);
    assert_eq!(events[1].sequence, 3);
    assert_eq!(events[1].event, app_reloaded);
    assert!(matches!(
        events[0].event,
        VmCodeServerEvent::Published { .. }
    ));
    assert!(matches!(
        events[1].event,
        VmCodeServerEvent::HotReloaded { .. }
    ));

    assert!(code_server.snapshots_for_module("missing.Main").is_empty());
    assert!(code_server
        .event_snapshots_for_module("missing.Main")
        .is_empty());
}

#[test]
fn code_server_tracks_active_coreir_function_exports_across_reload() {
    let source_v1 = concat!(
        "module app.Main.\n\n",
        "pub value(input: Int): Int -> input.\n\n",
        "pub current(): Int -> 1.\n\n",
        "private_value(): Int -> 2.\n",
    );
    let source_v2 = concat!(
        "module app.Main.\n\n",
        "pub replacement(): Int -> 3.\n\n",
        "private_value(): Int -> 4.\n",
    );
    let mut code_server = VmCodeServer::default();

    code_server
        .publish_source("src/app/Main.terl", source_v1)
        .expect("first source generation should publish");
    assert!(code_server.module_loaded("app.Main"));
    assert!(!code_server.module_loaded("missing.Main"));
    assert!(code_server.function_exported("app.Main", "value", 1));
    assert!(code_server.function_exported("app.Main", "current", 0));
    assert!(!code_server.function_exported("app.Main", "value", 0));
    assert!(!code_server.function_exported("app.Main", "private_value", 0));
    assert!(!code_server.function_exported("missing.Main", "value", 1));

    code_server
        .publish_source("src/app/Main.terl", source_v2)
        .expect("replacement source generation should publish");
    assert!(!code_server.function_exported("app.Main", "value", 1));
    assert!(!code_server.function_exported("app.Main", "current", 0));
    assert!(code_server.function_exported("app.Main", "replacement", 0));
    assert!(!code_server.function_exported("app.Main", "private_value", 0));
}

#[test]
fn code_server_exposes_typed_active_module_metadata_without_beam_pseudo_exports() {
    let source = concat!(
        "module app.Metadata.\n\n",
        "pub value(input: Int): Int -> input.\n\n",
        "pub current(): Int -> helper().\n\n",
        "helper(): Int -> 17.\n",
    );
    let mut code_server = VmCodeServer::default();
    let event = code_server
        .publish_source("src/app/Metadata.terl", source)
        .expect("module metadata source should publish");
    let VmCodeServerEvent::Published { generation, .. } = event else {
        panic!("first module metadata generation must publish")
    };

    let before_missing_lookup = code_server
        .active_module_info("app.Metadata")
        .expect("active module metadata");
    assert!(!code_server.function_exported("app.Metadata", "missing", 0));
    let info = code_server
        .active_module_info("app.Metadata")
        .expect("stable active module metadata");

    assert_eq!(info, before_missing_lookup);
    assert_eq!(info.module, "app.Metadata");
    assert_eq!(info.generation, generation);
    assert!(info.checksum.starts_with("source-fnv1a64:"));
    assert!(info.source_map_id.starts_with("src/app/Metadata.terl:"));
    assert_eq!(
        info.exports,
        vec![
            VmModuleFunction {
                name: "current".to_string(),
                arity: 0,
            },
            VmModuleFunction {
                name: "value".to_string(),
                arity: 1,
            },
        ]
    );
    assert_eq!(
        info.functions,
        vec![
            VmModuleFunction {
                name: "current".to_string(),
                arity: 0,
            },
            VmModuleFunction {
                name: "helper".to_string(),
                arity: 0,
            },
            VmModuleFunction {
                name: "value".to_string(),
                arity: 1,
            },
        ]
    );
    assert!(info
        .functions
        .iter()
        .all(|function| function.name != "module_info"));
}

/// Replaces OTP's module-info load, export, unload, and purge fixture.
#[test]
fn code_server_replaces_module_info_lifecycle_fixture() {
    let source = concat!(
        "module module_info_test.\n\n",
        "pub f(): Int -> 17.\n\n",
        "hidden(): Int -> 19.\n",
    );
    let mut code_server = VmCodeServer::default();

    let published = code_server
        .publish_source("module_info_test.terl", source)
        .expect("module-info fixture should publish");
    let VmCodeServerEvent::Published { generation, .. } = published else {
        panic!("first module-info generation must publish");
    };
    assert!(code_server.module_loaded("module_info_test"));
    assert!(code_server.function_exported("module_info_test", "f", 0));
    assert!(!code_server.function_exported("module_info_test", "f", 1));
    assert!(!code_server.function_exported("module_info_test", "hidden", 0));

    assert_eq!(
        code_server
            .unload_active_generation("module_info_test")
            .expect("unbound module should unload"),
        VmCodeServerEvent::GenerationRetired {
            module: "module_info_test".to_string(),
            generation,
        }
    );
    assert!(!code_server.module_loaded("module_info_test"));
    assert!(!code_server.function_exported("module_info_test", "f", 0));
    assert!(code_server.active_module_info("module_info_test").is_err());
    assert_eq!(
        code_server.snapshots_for_module("module_info_test")[0].state,
        VmModuleGenerationState::Retired
    );

    assert_eq!(
        code_server
            .purge_retired_generations("module_info_test")
            .expect("retired module should purge"),
        vec![VmCodeServerEvent::GenerationPurged {
            module: "module_info_test".to_string(),
            generation,
        }]
    );
    assert!(code_server
        .snapshots_for_module("module_info_test")
        .is_empty());
    assert!(code_server.active_module_info("module_info_test").is_err());
}

/// Proves unload rejection preserves a process-bound active generation.
#[test]
fn code_server_rejects_unloading_process_bound_module_without_mutation() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(process_source("loop"));
    let mut code_server = VmCodeServer::default();
    code_server.publish("module_info_test", artifact("module-info"));
    code_server
        .bind_process_to_active(&processes, pid, "module_info_test")
        .expect("live process should bind to active module");
    let snapshots_before = code_server.snapshots_for_module("module_info_test");
    let events_before = code_server.event_snapshots_for_module("module_info_test");

    let error = code_server
        .unload_active_generation("module_info_test")
        .expect_err("process-bound module must not unload");

    assert_eq!(
        error,
        "cannot unload active generation 1 for module `module_info_test`: 1 process binding(s) remain"
    );
    assert_eq!(
        code_server.snapshots_for_module("module_info_test"),
        snapshots_before
    );
    assert_eq!(
        code_server.event_snapshots_for_module("module_info_test"),
        events_before
    );
    assert!(code_server.module_loaded("module_info_test"));
}
