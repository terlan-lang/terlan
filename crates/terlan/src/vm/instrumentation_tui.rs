//! Ratatui renderer for the local Terlan VM dashboard.
//!
//! Inputs:
//! - Validated VM instrumentation dashboard configuration.
//!
//! Outputs:
//! - Read-only Ratatui terminal frame content.
//!
//! Transformation:
//! - Adapts provider-neutral VM dashboard descriptors to terminal widgets
//!   without creating a separate dashboard state model.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::instrumentation::{
    validate_vm_dashboard_config, VmDashboardConfig, VmInstrumentationDiagnostic,
};

/// Renders the local VM dashboard into one Ratatui frame.
///
/// Inputs:
/// - `frame`: Ratatui frame for the active terminal draw pass.
/// - `config`: local VM dashboard descriptor.
///
/// Output:
/// - `Ok(())` when the config is read-only and locally scoped.
/// - Stable diagnostics when validation rejects the dashboard.
///
/// Transformation:
/// - Uses the provider, mode, and component descriptors from
///   `vm::instrumentation`; the terminal layer only chooses presentation.
#[cfg(test)]
pub(crate) fn render_local_vm_dashboard(
    frame: &mut Frame<'_>,
    config: &VmDashboardConfig,
) -> Result<(), Vec<VmInstrumentationDiagnostic>> {
    match validate_vm_dashboard_config(config) {
        Ok(()) => {
            render_dashboard_widget(frame, " Terlan VM ", dashboard_lines(config));
            Ok(())
        }
        Err(diagnostics) => {
            render_dashboard_widget(
                frame,
                " Terlan VM Error ",
                dashboard_error_lines(config, &diagnostics),
            );
            Err(diagnostics)
        }
    }
}

/// Builds the Ratatui text lines for one dashboard.
///
/// Inputs:
/// - `config`: validated dashboard descriptor.
///
/// Output:
/// - Ordered Ratatui lines for provider, mode, and component inventory.
///
/// Transformation:
/// - Converts typed component identity into a compact read-only terminal
///   summary while preserving stable ids for tests and future CLI plumbing.
#[cfg(test)]
fn dashboard_lines(config: &VmDashboardConfig) -> Vec<Line<'static>> {
    let mut lines = vec![
        styled_pair("Provider", config.provider.display_name),
        styled_pair("Provider ID", config.provider.id),
        styled_pair("Scope", config.provider.scope.as_str()),
        styled_pair("Transport", config.provider.transport.as_str()),
        styled_pair("Mode", config.mode.as_str()),
        Line::from(""),
        Line::from(Span::styled(
            "Components",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];

    lines.extend(config.components.iter().map(|component| {
        Line::from(format!(
            "- {} | {} | {}",
            component.id,
            component.title,
            component.kind.as_str()
        ))
    }));
    if config.components.is_empty() {
        lines.push(Line::from("- no VM components registered"));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Operator controls: disabled"));
    lines
}

/// Builds the Ratatui text lines for a validation error dashboard.
///
/// Inputs:
/// - `config`: dashboard descriptor that failed validation.
/// - `diagnostics`: stable validation diagnostics.
///
/// Output:
/// - Ordered Ratatui lines that show the failure and the disabled operator
///   state.
///
/// Transformation:
/// - Keeps invalid dashboards visible for local debugging without weakening
///   the runtime validation contract.
#[cfg(test)]
fn dashboard_error_lines(
    config: &VmDashboardConfig,
    diagnostics: &[VmInstrumentationDiagnostic],
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Dashboard unavailable",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        styled_pair("Provider", config.provider.display_name),
        styled_pair("Mode", config.mode.as_str()),
        Line::from(""),
    ];

    if config.components.is_empty() {
        lines.push(Line::from("Components: no VM components registered"));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "Diagnostics",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.extend(diagnostics.iter().map(|diagnostic| {
        Line::from(format!(
            "error[{}]: {}",
            diagnostic.code, diagnostic.message
        ))
    }));
    lines.push(Line::from(""));
    lines.push(Line::from("Operator controls: disabled"));
    lines
}

/// Renders a list of dashboard lines into the full terminal frame.
#[cfg(test)]
fn render_dashboard_widget(frame: &mut Frame<'_>, title: &'static str, lines: Vec<Line<'static>>) {
    let widget = Paragraph::new(lines).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(widget, frame.area());
}

/// Builds one bold-label dashboard line.
#[cfg(test)]
fn styled_pair(label: &'static str, value: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

#[cfg(test)]
#[path = "instrumentation_tui_test.rs"]
#[cfg(test)]
mod instrumentation_tui_test;
