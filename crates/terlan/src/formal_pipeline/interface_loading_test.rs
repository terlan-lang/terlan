use super::*;

/// A cached local provider must retain its canonical standard-library atom tags.
#[test]
fn scoped_loading_follows_cached_constructor_providers_to_atom_dependencies() {
    let root = tempfile::tempdir().unwrap();
    let summaries = root.path().join("std/summaries");
    let cache = root.path().join(".terlan");
    std::fs::create_dir_all(&summaries).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    let tags = include_str!("../../../../tests/fixtures/constructor-atom-alias/Tags.terl")
        .replace("fixture.Tags", "std.fixture.Tags");
    let tags = crate::terlan_syntax::parse_module_as_syntax_output(&tags).unwrap();
    let tags = crate::terlan_hir::syntax_module_output_to_interface(&tags);
    std::fs::write(
        summaries.join("std.fixture.Tags.typi"),
        tags.to_terlan_interface_text(),
    )
    .unwrap();
    let provider = include_str!("../../../../tests/fixtures/constructor-atom-alias/State.terl")
        .replace("fixture.Tags", "std.fixture.Tags");
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(&provider).unwrap();
    let interface = crate::terlan_hir::syntax_module_output_to_interface(&syntax);
    std::fs::write(
        cache.join("fixture.State.typi"),
        interface.to_terlan_interface_text(),
    )
    .unwrap();
    let consumer = crate::terlan_syntax::parse_module_as_syntax_output(include_str!(
        "../../../../tests/fixtures/constructor-atom-alias/StateTest.terl"
    ))
    .unwrap();
    let active = root.path().join("std/tests/StateTest.terl");
    let interfaces =
        load_external_interfaces_for_module(&active.to_string_lossy(), Some(&cache), &consumer);
    assert!(
        interfaces.contains_key("std.fixture.Tags"),
        "the constructor provider's canonical atom dependency was omitted"
    );
    assert!(!interfaces.contains_key("std.db.Postgres"));
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&consumer, &interfaces)
            .module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&consumer, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Cache sidecars cannot change the dependency closure of reviewed interfaces.
#[test]
fn shipped_interface_dependencies_ignore_missing_stale_and_corrupt_sidecars() {
    let root = tempfile::tempdir().unwrap();
    let summaries = root.path().join("std/summaries");
    std::fs::create_dir_all(&summaries).unwrap();
    let entry = summaries.join("std.fixture.Entry.typi");
    std::fs::write(
        &entry,
        "module std.fixture.Entry.\npub value(): std.fixture.Value.\n",
    )
    .unwrap();
    for name in ["Value", "Other"] {
        std::fs::write(
            summaries.join(format!("std.fixture.{name}.typi")),
            format!("module std.fixture.{name}.\npub opaque type {name}.\n"),
        )
        .unwrap();
    }
    let active = root.path().join("std/caller.terl");
    for sidecar in [None, Some("corrupt"), Some("deps=1\nstd.fixture.Other=0\n")] {
        if let Some(text) = sidecar {
            std::fs::write(entry.with_extension("typi.deps"), text).unwrap();
        }
        let mut interfaces = HashMap::new();
        load_required_standard_interfaces(
            &active.to_string_lossy(),
            BTreeSet::from(["std.fixture.Entry".into()]),
            &mut interfaces,
        );
        assert!(interfaces.contains_key("std.fixture.Value"));
        assert!(!interfaces.contains_key("std.fixture.Other"));
    }
    // Reusing the same path after an API edit must follow the new bytes.
    std::fs::write(
        &entry,
        "module std.fixture.Entry.\npub value(): std.fixture.Other.\n",
    )
    .unwrap();
    let mut interfaces = HashMap::new();
    load_required_standard_interfaces(
        &active.to_string_lossy(),
        BTreeSet::from(["std.fixture.Entry".into()]),
        &mut interfaces,
    );
    assert!(interfaces.contains_key("std.fixture.Other"));
    assert!(!interfaces.contains_key("std.fixture.Value"));
    std::fs::write(&entry, "invalid interface").unwrap();
    let mut interfaces = HashMap::new();
    load_required_standard_interfaces(
        &active.to_string_lossy(),
        BTreeSet::from(["std.fixture.Entry".into()]),
        &mut interfaces,
    );
    assert!(!interfaces.contains_key("std.fixture.Entry"));
}

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

/// Embedded interfaces reference embedded or packaged standard providers.
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
    // Generated DOM interfaces are intentionally shipped on disk; embedding
    // two entry points does not embed their entire generated type graph.
    let packaged = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std/summaries");
    let mut available = modules
        .iter()
        .map(|name| name.to_string())
        .collect::<BTreeSet<_>>();
    for path in std::fs::read_dir(packaged)
        .unwrap()
        .map(|entry| entry.unwrap().path())
    {
        if path
            .extension()
            .is_some_and(|extension| extension == "typi")
        {
            available.insert(path.file_stem().unwrap().to_string_lossy().into_owned());
        }
    }
    let mut missing = Vec::new();
    for entry in EMBEDDED_STD_INTERFACES {
        if entry.namespace {
            let mut lines = entry.summary.lines();
            assert_eq!(lines.next(), Some(entry.module));
            for line in lines {
                let child = line.strip_prefix("module ").expect("namespace child");
                assert!(modules.contains(format!("{}.{child}", entry.module).as_str()));
            }
            continue;
        }
        let (name, interface) = cached_embedded_std_interface(entry.summary)
            .unwrap_or_else(|| panic!("invalid embedded interface {}", entry.module));
        assert_eq!(name, entry.module);
        // Bare names in constructor bodies are source-local aliases, not
        // catalog identities. Qualified references must resolve to providers.
        for dependency in interface
            .dependency_modules
            .iter()
            .filter(|name| name.contains('.'))
        {
            // Qualified nominal references can name either a provider module
            // or its exported type. Resolve the longest available provider.
            let mut provider = dependency.as_str();
            while !available.contains(provider) {
                let Some((parent, _)) = provider.rsplit_once('.') else {
                    break;
                };
                provider = parent;
            }
            let namespace_candidate = interface.dependency_modules.iter().any(|candidate| {
                candidate.starts_with(&format!("{dependency}."))
                    && available.contains(candidate.as_str())
            });
            if !available.contains(provider) && !namespace_candidate {
                missing.push(format!(
                    "{} requires unavailable {dependency}",
                    entry.module
                ));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
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

#[test]
fn scoped_loading_admits_shared_entropy_and_encoding_interfaces() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        "module scoped_entropy_interfaces. import std.crypto.Entropy. import std.encoding.Base64.
         pub token(): String -> Base64.encode_url_unpadded_bytes(Entropy.bytes(32)).",
    )
    .unwrap();
    let interfaces = load_external_interfaces_for_module("fixture.terl", None, &module);
    assert!(interfaces.contains_key("std.crypto.Entropy"));
    assert!(interfaces.contains_key("std.encoding.Base64"));
}

/// Primary type references must load their provider without an explicit import.
#[test]
fn scoped_loading_admits_primary_types_without_imports() {
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        r#"module scoped_primary_types.
        pub echo(value: std.vm.Bytes): std.vm.Bytes -> value.
        pub literal(): String -> "std.db.Postgres"."#,
    )
    .expect("parse primary type fixture");
    let interfaces = load_external_interfaces_for_module("fixture.terl", None, &module);
    assert!(interfaces.contains_key("std.vm.Bytes"));
    assert!(!interfaces.contains_key("std.db.Postgres"));
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&module, &interfaces)
            .module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "primary type dependency: {diagnostics:?}"
    );
}
