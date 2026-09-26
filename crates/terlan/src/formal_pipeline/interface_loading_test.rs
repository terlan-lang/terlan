use super::*;

/// Installed compilation must include nominal types referenced by imported helpers.
#[test]
fn scoped_loading_typechecks_table_helpers_with_transitive_iterator_dependency() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(include_str!(
        "../../../../std/test/TableTest.terl"
    ))
    .expect("parse canonical table suite");
    let interfaces = load_external_interfaces_for_module("fixture.terl", None, &module);
    assert!(interfaces.contains_key("std.collections.Iterator"));
    assert!(!interfaces.contains_key("std.db.Postgres"));
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&module, &interfaces)
            .module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "installed table contract: {diagnostics:?}"
    );
}

/// Every embedded dependency is available without consulting checkout files.
#[test]
fn embedded_catalog_has_valid_complete_dependency_closure() {
    let modules = EMBEDDED_STD_INTERFACES
        .iter()
        .map(|entry| entry.module)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        modules.len(),
        EMBEDDED_STD_INTERFACES.len(),
        "duplicate module identity"
    );
    for entry in EMBEDDED_STD_INTERFACES {
        let Some(manifest) = entry.dependencies else {
            let mut lines = entry.summary.lines();
            assert_eq!(lines.next(), Some(entry.module));
            for line in lines {
                let child = line.strip_prefix("module ").expect("namespace child");
                assert!(modules.contains(format!("{}.{child}", entry.module).as_str()));
            }
            continue;
        };
        let (name, _) = cached_embedded_std_interface(entry.summary)
            .unwrap_or_else(|| panic!("invalid embedded interface {}", entry.module));
        assert_eq!(name, entry.module);
        let dependencies =
            parse_interface_dependency_entries(manifest).expect("valid dependency manifest");
        for (dependency, _) in dependencies {
            assert!(
                modules.contains(dependency.as_str()),
                "{} requires unavailable {dependency}",
                entry.module
            );
        }
    }
}

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
