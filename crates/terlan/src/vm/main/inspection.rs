use super::instrumentation::{
    default_local_vm_instrumentation_provider, vm_runtime_inspection_snapshot,
    VmProcessInspectionSnapshot, VmResourceInspectionSnapshot, VmRuntimeInspectionSnapshot,
    VmSupervisorInspectionSnapshot,
};
use super::VmInspectSubject;

/// Inspects the current standalone VM through the typed local provider.
pub(super) fn inspect_local_vm(subject: VmInspectSubject) -> Result<String, String> {
    let snapshot = empty_local_vm_inspection_snapshot()?;
    render_inspection_subject(&snapshot, subject)
}

pub(super) fn empty_local_vm_inspection_snapshot() -> Result<VmRuntimeInspectionSnapshot, String> {
    vm_runtime_inspection_snapshot(
        &default_local_vm_instrumentation_provider(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(super) fn render_inspection_subject(
    snapshot: &VmRuntimeInspectionSnapshot,
    subject: VmInspectSubject,
) -> Result<String, String> {
    match subject {
        VmInspectSubject::Processes => Ok(render_processes(snapshot)),
        VmInspectSubject::Supervisors => Ok(render_supervisors(snapshot)),
        VmInspectSubject::Resources => Ok(render_resources(snapshot)),
        VmInspectSubject::Process { pid } => render_process(snapshot, &pid),
    }
}

fn render_processes(snapshot: &VmRuntimeInspectionSnapshot) -> String {
    let mut lines = vec![
        format!("provider: {}", snapshot.provider_id),
        "pid state mailbox ticks heap source".to_string(),
    ];
    if snapshot.processes.is_empty() {
        lines.push("no processes".to_string());
    } else {
        lines.extend(snapshot.processes.iter().map(render_process_row));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_supervisors(snapshot: &VmRuntimeInspectionSnapshot) -> String {
    let mut lines = vec![
        format!("provider: {}", snapshot.provider_id),
        "id strategy restarts children".to_string(),
    ];
    if snapshot.supervisors.is_empty() {
        lines.push("no supervisors".to_string());
    } else {
        lines.extend(snapshot.supervisors.iter().map(render_supervisor_row));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_resources(snapshot: &VmRuntimeInspectionSnapshot) -> String {
    let mut lines = vec![
        format!("provider: {}", snapshot.provider_id),
        "handle owner kind state".to_string(),
    ];
    if snapshot.resources.is_empty() {
        lines.push("no resources".to_string());
    } else {
        lines.extend(snapshot.resources.iter().map(render_resource_row));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_process(snapshot: &VmRuntimeInspectionSnapshot, pid: &str) -> Result<String, String> {
    let process = snapshot
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .ok_or_else(|| format!("error[vm_inspect_not_found]: process `{pid}` was not found"))?;
    Ok(format!(
        "provider: {}\n{}\n",
        snapshot.provider_id,
        render_process_detail(process)
    ))
}

fn render_process_row(process: &VmProcessInspectionSnapshot) -> String {
    format!(
        "{} {} {} {} {} {}.{}",
        process.pid,
        process.state.as_str(),
        process.mailbox_len,
        process.reductions,
        process.heap_bytes,
        process.source_module,
        process.source_function
    )
}

fn render_supervisor_row(supervisor: &VmSupervisorInspectionSnapshot) -> String {
    format!(
        "{} {} {} {}",
        supervisor.id,
        supervisor.strategy,
        supervisor.restart_count,
        supervisor.child_pids.join(",")
    )
}

fn render_resource_row(resource: &VmResourceInspectionSnapshot) -> String {
    format!(
        "{} {} {} {}",
        resource.handle, resource.owner_pid, resource.kind, resource.state
    )
}

fn render_process_detail(process: &VmProcessInspectionSnapshot) -> String {
    format!(
        "pid: {}\nparent: {}\nsupervisor: {}\nstate: {}\nmailbox: {}\nticks: {}\nheap_bytes: {}\nrestarts: {}\nresources: {}\nnative_call: {}\ncancellation_requested: {}\nsource: {}.{}",
        process.pid,
        process.parent_pid.as_deref().unwrap_or("none"),
        process.supervisor_id.as_deref().unwrap_or("none"),
        process.state.as_str(),
        process.mailbox_len,
        process.reductions,
        process.heap_bytes,
        process.restart_count,
        empty_join(&process.resource_handles),
        process.native_call_state.as_deref().unwrap_or("none"),
        process.cancellation_requested,
        process.source_module,
        process.source_function
    )
}

fn empty_join(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}
