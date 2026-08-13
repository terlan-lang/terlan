//! Dynamic debugger trace-filter matching.

use std::collections::BTreeSet;

use crate::runtime::native_image::control::TvmTransitionOperation;

#[cfg(test)]
#[path = "tracing_test.rs"]
mod test;

const EVENT_KINDS: &[&str] = &[
    "calls",
    "returns",
    "transitions",
    "sends",
    "receives",
    "mailbox",
    "processes",
    "resources",
    "native_boundary",
    "http",
    "supervisors",
];

pub(super) fn event_enabled(
    filters: &BTreeSet<String>,
    kind: &str,
    process: u64,
    module: &str,
    function: &str,
) -> bool {
    if filters.is_empty() {
        return false;
    }
    let kind_selected = filters.contains(kind)
        || !filters
            .iter()
            .any(|filter| EVENT_KINDS.contains(&filter.as_str()));
    kind_selected
        && qualifier_matches(filters, "process:", &process.to_string())
        && qualifier_matches(filters, "module:", module)
        && qualifier_matches(filters, "function:", function)
}

pub(super) fn resource_enabled(filters: &BTreeSet<String>, kind: &str) -> bool {
    filters.contains("resources") && qualifier_matches(filters, "resource:", kind)
}

/// Renders filters over a transition emitted by generated AOT code.
pub(super) fn transition_events(
    filters: &BTreeSet<String>,
    operation: TvmTransitionOperation,
    process: u64,
    module: &str,
    function: &str,
    arguments: &[i64],
) -> Vec<String> {
    let mut events = Vec::new();
    let selected = |kind| event_enabled(filters, kind, process, module, function);
    match operation {
        TvmTransitionOperation::Send if selected("sends") => {
            let recipient = arguments.first().copied().unwrap_or_default();
            let shape = arguments.last().copied().unwrap_or_default();
            if message_matches(filters, shape) {
                events.push(format!("trace:send:{process}:{recipient}:{shape}"));
            }
        }
        TvmTransitionOperation::Receive => {
            if selected("receives") {
                events.push(format!("trace:receive:{process}:waiting"));
            }
            if selected("mailbox") {
                events.push(format!("trace:mailbox_match:{process}:selective"));
            }
        }
        TvmTransitionOperation::Spawn if selected("processes") => {
            events.push(format!("trace:process:{process}:spawn"));
        }
        TvmTransitionOperation::Resource if selected("resources") => {
            let kind = arguments.first().copied().unwrap_or_default();
            events.push(format!("trace:resource:{process}:acquire:{kind}"));
        }
        TvmTransitionOperation::Capability => {
            if selected("native_boundary") {
                events.push(format!("trace:native_boundary:{process}:capability"));
            }
            if selected("http") {
                events.push(format!("trace:http:{process}:capability"));
            }
        }
        TvmTransitionOperation::Failure if selected("processes") => {
            events.push(format!("trace:process:{process}:failure"));
        }
        _ => {}
    }
    events
}

fn qualifier_matches(filters: &BTreeSet<String>, prefix: &str, value: &str) -> bool {
    let qualifiers = filters
        .iter()
        .filter_map(|filter| filter.strip_prefix(prefix))
        .collect::<Vec<_>>();
    qualifiers.is_empty() || qualifiers.contains(&value)
}

fn message_matches(filters: &BTreeSet<String>, shape: i64) -> bool {
    qualifier_matches(filters, "message:", &shape.to_string())
}
