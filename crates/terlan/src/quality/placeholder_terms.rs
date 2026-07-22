/// Build placeholder-term diagnostics for quality report evidence entries.
///
/// Inputs:
/// - `label`: evidence bucket name used in diagnostics.
/// - `entries`: report entries to scan.
/// - `placeholder_terms`: lower-case placeholder vocabulary.
/// - `render`: diagnostic renderer preserving each report's wording.
///
/// Outputs:
/// - Diagnostics for entries containing any placeholder term.
///
/// Transformation:
/// - Lower-cases each entry for matching while preserving the original entry
///   text in diagnostics.
pub(crate) fn placeholder_entry_diagnostics(
    label: &str,
    entries: &[&str],
    placeholder_terms: &[&str],
    render: impl Fn(&str, &str, &str) -> String,
) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            placeholder_terms
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| render(label, entry, term))
        })
        .collect()
}

/// Build placeholder-term diagnostics for editor selector evidence.
///
/// Inputs:
/// - `selectors`: static selector fixture names paired with report evidence.
/// - `report_notes`: report-level notes to scan after selector evidence.
/// - `validator`: report-specific placeholder validator preserving wording.
///
/// Outputs:
/// - Diagnostics for selector evidence and report notes.
///
/// Transformation:
/// - Converts selector fixture names into stable evidence labels and delegates
///   term matching to the caller-supplied validator.
pub(crate) fn selector_evidence_placeholder_diagnostics(
    selectors: impl IntoIterator<Item = (&'static str, &'static [&'static str])>,
    report_notes: &[&str],
    validator: impl Fn(&str, &[&str]) -> Vec<String>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (fixture, evidence) in selectors {
        diagnostics.extend(validator(
            &format!("selector `{fixture}` evidence"),
            evidence,
        ));
    }
    diagnostics.extend(validator("editor parity notes", report_notes));
    diagnostics
}
