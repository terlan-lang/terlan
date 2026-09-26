use super::{inspect_local_vm, rendering::render_inspection_subject, VmInspectSubject};
use crate::vm::instrumentation::{
    default_local_vm_instrumentation_provider, vm_runtime_inspection_snapshot,
};

#[test]
fn standalone_inspection_rejects_all_subjects_without_a_live_connection() {
    for subject in [
        VmInspectSubject::Processes,
        VmInspectSubject::Supervisors,
        VmInspectSubject::Resources,
        VmInspectSubject::Process { pid: "123".into() },
    ] {
        let error = inspect_local_vm(subject).expect_err("must not manufacture an empty snapshot");
        assert!(error.starts_with("error[vm_inspect_unavailable]"));
        assert!(error.contains("no runtime snapshot was collected"));
    }
}

#[test]
fn retained_renderer_distinguishes_empty_snapshots_from_missing_processes() {
    let snapshot = vm_runtime_inspection_snapshot(
        &default_local_vm_instrumentation_provider(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .expect("explicit snapshot");
    for (subject, expected) in [
        (VmInspectSubject::Processes, "no processes"),
        (VmInspectSubject::Supervisors, "no supervisors"),
        (VmInspectSubject::Resources, "no resources"),
    ] {
        assert!(render_inspection_subject(&snapshot, subject)
            .expect("render")
            .contains(expected));
    }
    assert!(
        render_inspection_subject(&snapshot, VmInspectSubject::Process { pid: "123".into() })
            .expect_err("missing process")
            .contains("error[vm_inspect_not_found]")
    );
}
