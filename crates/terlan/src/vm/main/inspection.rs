use super::VmInspectSubject;

/// Standalone inspection cannot observe another VM until an attachment transport exists.
pub(super) fn inspect_local_vm(subject: VmInspectSubject) -> Result<String, String> {
    let selected = match subject {
        VmInspectSubject::Processes => "processes".to_string(),
        VmInspectSubject::Supervisors => "supervisors".to_string(),
        VmInspectSubject::Resources => "resources".to_string(),
        VmInspectSubject::Process { pid } => format!("process {pid}"),
    };
    Err(format!(
        "error[vm_inspect_unavailable]: live VM inspection is not connected for {selected}; no runtime snapshot was collected"
    ))
}

// Retained renderer for the planned live inspection integration.
#[cfg(test)]
#[path = "inspection_rendering.rs"]
mod rendering;

#[cfg(test)]
#[path = "inspection_test.rs"]
mod tests;
