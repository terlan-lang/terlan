use std::collections::HashSet;

use super::{resolve_scoped_call, FunctionKey};

#[test]
fn unqualified_call_resolves_only_through_the_callers_imports() {
    let providers = vec![
        function("std.alpha.Codec", "encode_exact", 2),
        function("std.binary.Binary", "encode_exact", 2),
    ];
    let imported = HashSet::from(["std.binary.Binary"]);

    assert_eq!(
        resolve_scoped_call("app.Main", &imported, "encode_exact", 2, &providers),
        Some(function("std.binary.Binary", "encode_exact", 2))
    );
}

#[test]
fn unqualified_call_resolves_through_a_symbol_import() {
    let providers = vec![
        function("std.alpha.Codec", "encode_exact", 2),
        function("std.binary.Binary", "encode_exact", 2),
    ];
    let imported = HashSet::from(["std.binary.Binary.encode_exact"]);

    assert_eq!(
        resolve_scoped_call("app.Main", &imported, "encode_exact", 2, &providers),
        Some(function("std.binary.Binary", "encode_exact", 2))
    );
}

#[test]
fn qualified_call_resolves_without_an_import() {
    let providers = vec![function("std.binary.Binary", "encode_exact", 2)];

    assert_eq!(
        resolve_scoped_call(
            "app.Main",
            &HashSet::new(),
            "std.binary.Binary.encode_exact",
            2,
            &providers,
        ),
        Some(function("std.binary.Binary", "encode_exact", 2))
    );
}

fn function(module: &str, name: &str, arity: usize) -> FunctionKey {
    (module.to_string(), name.to_string(), arity)
}
