//! Qualified constructor resolution must survive colliding local spellings.

use super::*;

#[test]
fn constructor_resolution_retains_provider_identity_and_module_facades() {
    let identities = HashMap::from([("List".into(), "std.collections.List.List".into())]);
    for (prior, expected) in [
        (None, "std.collections.List.List"),
        (Some("std.collections.List"), "std.collections.List.List"),
        (
            Some("std.collections.List.List"),
            "std.collections.List.List",
        ),
        (Some("app.Containers.List"), "app.Containers.List"),
    ] {
        let mut resolved = prior.map(str::to_string);
        retain_constructor_identity("List", &mut resolved, &identities);
        assert_eq!(resolved.as_deref(), Some(expected));
        retain_constructor_identity("List", &mut resolved, &identities);
        assert_eq!(resolved.as_deref(), Some(expected));
    }
}
