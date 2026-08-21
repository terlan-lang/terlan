use super::*;

#[test]
fn scoped_loading_admits_imports_without_unrelated_std_modules() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        "module scoped_interfaces.\nimport std.core.{String}.\nimport std.data.Json.\npub run(): Json -> Json.string(String.append(\"a\", \"b\")).\n",
    )
    .expect("parse scoped interface fixture");

    let interfaces = load_external_interfaces_for_module("fixture.terl", None, &module);

    assert!(interfaces.contains_key("std.data.Json"));
    assert!(interfaces.contains_key("std.core.String"));
    assert!(!interfaces.contains_key("std.db.Postgres"));
}

/// Verifies script-style module-default imports load their concrete child
/// interfaces instead of stopping at namespace prefixes.
#[test]
fn scoped_loading_expands_module_default_value_and_type_imports() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        "module scoped_script_interfaces.\n\
         import std.core.{String}.\n\
         import std.data.Json.\n\
         import type std.data.Json.\n\
         pub run(): Json -> Json.string(String.append(\"a\", \"b\")).\n",
    )
    .expect("parse script interface fixture");

    let interfaces = load_external_interfaces_for_module("fixture.terls", None, &module);

    assert!(interfaces.contains_key("std.core.String"));
    assert!(interfaces.contains_key("std.data.Json"));
    assert!(!interfaces.contains_key("std.data"));
}

#[test]
fn scoped_loading_admits_fully_qualified_remote_modules_without_imports() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        "module scoped_remote_interfaces.\n\
         pub run(): String -> std.crypto.Hash.sha256(\"abc\").\n",
    )
    .expect("parse fully qualified remote fixture");

    let interfaces = load_external_interfaces_for_module("fixture.terl", None, &module);

    assert!(interfaces.contains_key("std.crypto.Hash"));
    assert!(!interfaces.contains_key("std.db.Postgres"));
}
