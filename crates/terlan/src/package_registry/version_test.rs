use super::*;

#[test]
fn semantic_order_is_not_lexical_and_prereleases_are_not_stable() {
    assert_eq!(
        latest_stable(["1.9.0", "1.10.0", "2.0.0-rc.1"].into_iter()),
        Some("1.10.0".into())
    );
    assert!(canonical_version("01.0.0").is_err());
    assert!(parse_requirement("definitely not semver").is_err());
    assert!(requirement_matches(">=1.9.0, <2.0.0", "1.10.0").unwrap());
    assert!(!requirement_matches(">=2.0.0", "1.10.0").unwrap());
}
