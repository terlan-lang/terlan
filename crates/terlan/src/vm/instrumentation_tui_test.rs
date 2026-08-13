use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

use super::render_local_vm_dashboard;
use crate::vm::instrumentation::{
    cloud_vm_instrumentation_provider, default_local_vm_dashboard_config,
    vm_dashboard_render_snapshot, VmDashboardMode,
};

/// Verifies the Ratatui renderer consumes the typed local dashboard config.
///
/// Inputs:
/// - Default local VM dashboard configuration.
/// - Ratatui test backend.
///
/// Output:
/// - Test passes when provider, read-only mode, component ids, and disabled
///   operator state render into the terminal buffer.
///
/// Transformation:
/// - Exercises the real Ratatui frame renderer without opening an interactive
///   terminal.
#[test]
fn ratatui_renderer_renders_default_local_dashboard() {
    let config = default_local_vm_dashboard_config();
    let text = render_dashboard_text(&config);
    let snapshot =
        vm_dashboard_render_snapshot(&config, text.clone()).expect("valid dashboard snapshot");

    assert_eq!(snapshot.provider_id, "local.vm");
    assert_eq!(snapshot.mode, "read_only");
    assert_eq!(
        snapshot.component_ids,
        ["runtime", "processes", "queues", "native-boundary"]
    );
    assert!(text.contains("Terlan VM"));
    assert!(text.contains("Provider: Local Terlan VM"));
    assert!(text.contains("Mode: read_only"));
    assert!(text.contains("runtime | Runtime | runtime_overview"));
    assert!(text.contains("processes | Processes | process_list"));
    assert!(text.contains("queues | Queues | message_queues"));
    assert!(text.contains("native-boundary | Native Boundary | native_boundary"));
    assert!(text.contains("Operator controls: disabled"));
}

/// Verifies the Ratatui renderer rejects cloud providers for the local TUI.
///
/// Inputs:
/// - Default dashboard config with provider replaced by cloud descriptor.
///
/// Output:
/// - Test passes when local rendering returns the provider validation
///   diagnostics instead of drawing remote state.
///
/// Transformation:
/// - Pins the boundary that local terminal dashboards do not depend on Terlan
///   Cloud transports.
#[test]
fn ratatui_renderer_rejects_cloud_provider_for_local_dashboard() {
    let mut config = default_local_vm_dashboard_config();
    config.provider = cloud_vm_instrumentation_provider();
    let (diagnostics, text) = render_dashboard_error_text(&config);

    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&"vm_instrumentation_non_local_scope"));
    assert!(codes.contains(&"vm_instrumentation_non_local_transport"));
    assert!(text.contains("Terlan VM Error"));
    assert!(text.contains("Dashboard unavailable"));
    assert!(text.contains("error[vm_instrumentation_non_local_scope]"));
    assert!(text.contains("error[vm_instrumentation_non_local_transport]"));
    assert!(text.contains("Operator controls: disabled"));
}

/// Verifies empty component sets render a stable empty dashboard state.
///
/// Inputs:
/// - Local dashboard config with no components.
/// - Ratatui test backend.
///
/// Output:
/// - Test passes when the terminal buffer includes the empty-state text and
///   the validation diagnostic.
///
/// Transformation:
/// - Keeps validation strict while ensuring local users see a useful terminal
///   state for incomplete instrumentation providers.
#[test]
fn ratatui_renderer_renders_empty_component_error_state() {
    let mut config = default_local_vm_dashboard_config();
    config.components.clear();
    let (diagnostics, text) = render_dashboard_error_text(&config);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_dashboard_missing_components");
    assert!(text.contains("Components: no VM components registered"));
    assert!(text.contains("error[vm_dashboard_missing_components]"));
    assert!(text.contains("Operator controls: disabled"));
}

/// Verifies operator mode stays visible but disabled in the TUI.
///
/// Inputs:
/// - Local dashboard config switched to operator mode.
/// - Ratatui test backend.
///
/// Output:
/// - Test passes when the terminal buffer shows operator mode and the disabled
///   operator diagnostic.
///
/// Transformation:
/// - Pins the v1 dashboard contract: operator actions are modeled elsewhere,
///   but the terminal dashboard remains read-only.
#[test]
fn ratatui_renderer_models_operator_mode_as_disabled() {
    let mut config = default_local_vm_dashboard_config();
    config.mode = VmDashboardMode::Operator;
    let (diagnostics, text) = render_dashboard_error_text(&config);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_dashboard_operator_mode_disabled");
    assert!(text.contains("Mode: operator"));
    assert!(text.contains("error[vm_dashboard_operator_mode_disabled]"));
    assert!(text.contains("Operator controls: disabled"));
}

/// Renders one dashboard to a plain text buffer.
fn render_dashboard_text(config: &crate::vm::instrumentation::VmDashboardConfig) -> String {
    let backend = TestBackend::new(96, 16);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_local_vm_dashboard(frame, config).expect("render dashboard"))
        .expect("draw dashboard");
    buffer_text(terminal.backend().buffer())
}

/// Renders one invalid dashboard and returns diagnostics plus terminal text.
fn render_dashboard_error_text(
    config: &crate::vm::instrumentation::VmDashboardConfig,
) -> (
    Vec<crate::vm::instrumentation::VmInstrumentationDiagnostic>,
    String,
) {
    let backend = TestBackend::new(112, 18);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut captured = None;
    terminal
        .draw(|frame| {
            captured =
                Some(render_local_vm_dashboard(frame, config).expect_err("invalid dashboard"));
        })
        .expect("draw invalid dashboard");
    (
        captured.expect("captured diagnostics"),
        buffer_text(terminal.backend().buffer()),
    )
}

/// Converts a Ratatui buffer to newline-separated text.
fn buffer_text(buffer: &Buffer) -> String {
    let area = *buffer.area();
    let mut lines = Vec::new();
    for y in area.y..area.y + area.height {
        let mut line = String::new();
        for x in area.x..area.x + area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}
