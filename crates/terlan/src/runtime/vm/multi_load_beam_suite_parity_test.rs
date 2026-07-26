//! VM publication replacements for portable `multi_load_SUITE` contracts.

use super::{
    VmCodeServer, VmCodeServerEvent, VmModuleArtifact, VmModuleGenerationState,
    VmStagedModuleArtifact,
};

const MODULES: usize = 100;

fn staged(module: impl Into<String>, revision: usize) -> VmStagedModuleArtifact {
    let module = module.into();
    VmStagedModuleArtifact {
        artifact: VmModuleArtifact::new(
            format!("{module}-checksum-{revision}"),
            format!("{module}-source-map-{revision}"),
        ),
        module,
    }
}

#[test]
fn multi_load_suite_publishes_one_closed_batch_of_distinct_modules() {
    let mut code_server = VmCodeServer::default();
    let staged = (0..MODULES)
        .map(|index| staged(format!("multi_load_{index:03}"), 1))
        .collect();

    assert!(
        code_server.snapshots().is_empty(),
        "staged modules must remain invisible before the batch publication"
    );
    let events = code_server
        .publish_staged_batch(staged)
        .expect("distinct module batch should publish");

    assert_eq!(events.len(), MODULES);
    assert!(events.iter().enumerate().all(|(index, event)| {
        matches!(
            event,
            VmCodeServerEvent::Published { module, generation }
                if module == &format!("multi_load_{index:03}")
                    && generation.as_u64() == index as u64 + 1
        )
    }));
    let snapshots = code_server.snapshots();
    assert_eq!(snapshots.len(), MODULES);
    assert!(snapshots
        .iter()
        .all(|snapshot| snapshot.state == VmModuleGenerationState::Active));
    assert_eq!(code_server.event_snapshots().len(), MODULES);
}

#[test]
fn multi_load_suite_duplicate_identity_rejects_the_entire_batch() {
    let mut code_server = VmCodeServer::default();
    code_server
        .publish_staged_batch(vec![staged("already_visible", 1)])
        .expect("seed batch");
    let snapshots_before = code_server.snapshots();
    let events_before = code_server.event_snapshots();

    let error = code_server
        .publish_staged_batch(vec![
            staged("first_new", 1),
            staged("duplicate", 1),
            staged("duplicate", 2),
            staged("last_new", 1),
        ])
        .expect_err("duplicate module identity must reject the closed batch");

    assert_eq!(
        error,
        "error[vm.code_server.duplicate_staged_module]: batch contains duplicate module `duplicate`"
    );
    assert_eq!(code_server.snapshots(), snapshots_before);
    assert_eq!(code_server.event_snapshots(), events_before);
}
