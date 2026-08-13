use super::*;

#[test]
fn scanner_tracks_scalar_array_and_target_locations() {
    let source = "schema = 1\ncapabilities = [\"tensor.compute\"]\nrequirements = [\n  \"tensor.storage\",\n]\n[[targets]]\ntriple = \"x86_64-unknown-linux-gnu\"\n";
    let spans = AcceleratorDescriptorSpans::scan(source, Path::new("pkg/accelerator.toml"));

    assert_eq!(spans.capability("tensor.compute").line, 2);
    assert_eq!(spans.capability("tensor.compute").column, 17);
    assert_eq!(spans.requirement("tensor.storage").line, 4);
    assert_eq!(spans.target("x86_64-unknown-linux-gnu").line, 7);
}

#[test]
fn diagnostics_use_codes_and_fallback_locations() {
    let spans = AcceleratorDescriptorSpans::default();
    let diagnostic = spans.diagnostic(
        "missing",
        spans.descriptor(),
        "missing capability".to_string(),
        Path::new("terlan.toml"),
    );

    assert_eq!(
        diagnostic,
        "error[accelerator.missing]: terlan.toml:1:1: missing capability"
    );
}
