use super::coverage_callable_id;

#[test]
fn source_identity_is_stable() {
    assert_eq!(
        coverage_callable_id("example.Web", "index", 1),
        coverage_callable_id("example.Web", "index", 1),
    );
}

#[test]
fn every_identity_component_affects_the_id() {
    let baseline = coverage_callable_id("example.Web", "index", 1);
    assert_ne!(baseline, coverage_callable_id("example.Api", "index", 1));
    assert_ne!(
        baseline,
        coverage_callable_id("example.Web", "health", 1)
    );
    assert_ne!(baseline, coverage_callable_id("example.Web", "index", 2));
}
