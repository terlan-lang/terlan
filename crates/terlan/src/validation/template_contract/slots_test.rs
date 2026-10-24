//! Template slots recognize supported primary and qualified type paths.

use super::{collection_inner_from_normalized_type, option_inner_from_normalized_type};

#[test]
fn template_type_text_accepts_primary_and_legacy_core_paths() {
    for ty in [
        "Option[Int]",
        "std.core.Option[Int]",
        "std.core.Option.Option[Int]",
    ] {
        assert_eq!(option_inner_from_normalized_type(ty), Some("Int"));
    }
    for ty in [
        "std.collections.List[Int]",
        "std.collections.List.List[Int]",
        "std.collections.Set[Int]",
        "std.collections.Set.Set[Int]",
    ] {
        assert_eq!(collection_inner_from_normalized_type(ty), Some("Int"));
    }
    assert_eq!(
        option_inner_from_normalized_type("custom.Option[Int]"),
        None
    );
}
