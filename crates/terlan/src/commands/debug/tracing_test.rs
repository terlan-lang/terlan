use std::collections::BTreeSet;

use super::event_enabled;

#[test]
fn trace_filters_intersect_event_and_identity_qualifiers() {
    let filters = ["calls", "process:7", "module:app.Main"]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    assert!(event_enabled(&filters, "calls", 7, "app.Main", "main"));
    assert!(!event_enabled(&filters, "returns", 7, "app.Main", "main"));
    assert!(!event_enabled(&filters, "calls", 8, "app.Main", "main"));
}
