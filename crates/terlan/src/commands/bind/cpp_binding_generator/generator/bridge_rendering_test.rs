//! Primary type matching preserves binding provider ownership.

use super::terlan_type_matches;

#[test]
fn canonical_binding_types_preserve_provider_ownership() {
    assert!(terlan_type_matches(
        "pytorch.Tensor",
        "pytorch.Tensor.Tensor"
    ));
    assert!(terlan_type_matches(
        "pytorch.Tensor.Tensor",
        "pytorch.Tensor"
    ));
    assert!(!terlan_type_matches(
        "pytorch.Tensor",
        "other.Tensor.Tensor"
    ));
    assert!(!terlan_type_matches(
        "polars.DataFrame",
        "polars.DataFrame.Series"
    ));
}
