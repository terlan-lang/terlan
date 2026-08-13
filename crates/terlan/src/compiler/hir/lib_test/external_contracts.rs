use super::*;

/// Verifies release core collection contracts produce stable interfaces.
///
/// Inputs:
/// - Release source contracts for `std.collections.Map`, `std.collections.List`, and
///   `std.collections.Set`.
/// - Matching release `.typi` summaries using bodyless receiver method
///   signatures.
///
/// Output:
/// - Test passes when source-contract extraction and summary parsing expose
///   the same key function arities, return types, and receiver mutability.
///
/// Transformation:
/// - Converts source and summary receiver methods into HIR's callable
///   `method(receiver, args...)` convention while preserving `mut`.
#[test]
pub(super) fn hir_extracts_release_core_collection_contracts_as_receiver_first_interfaces() {
    let contracts = [
        (
            "std.collections.Map",
            include_str!("../../../../../../std/collections/Map.terl"),
            include_str!("../../../../../../std/summaries/std.collections.Map.typi"),
            vec![
                ("put", 3, "Unit", "map", "Map[K, V]", true),
                ("remove", 2, "Unit", "map", "Map[K, V]", true),
                ("clear", 1, "Unit", "map", "Map[K, V]", true),
            ],
        ),
        (
            "std.collections.List",
            include_str!("../../../../../../std/collections/List.terl"),
            include_str!("../../../../../../std/summaries/std.collections.List.typi"),
            vec![
                ("push", 2, "Unit", "list", "List[T]", true),
                ("clear", 1, "Unit", "list", "List[T]", true),
            ],
        ),
        (
            "std.collections.Set",
            include_str!("../../../../../../std/collections/Set.terl"),
            include_str!("../../../../../../std/summaries/std.collections.Set.typi"),
            vec![
                ("add", 2, "Unit", "set", "Set[T]", true),
                ("remove", 2, "Unit", "set", "Set[T]", true),
                ("clear", 1, "Unit", "set", "Set[T]", true),
            ],
        ),
    ];

    for (module_name, source, summary, expected_functions) in contracts {
        let source_module =
            parse_module_as_syntax_output(source).expect("parse release collection source");
        let summary_module = parse_interface_module_as_syntax_output(summary)
            .expect("parse release collection summary");
        let source_interface = syntax_module_output_to_interface(&source_module);
        let summary_interface = syntax_module_output_to_interface(&summary_module);

        assert_eq!(source_interface.module, module_name);
        assert_eq!(summary_interface.module, module_name);

        for (function_name, arity, return_type, receiver_name, receiver_type, mutable) in
            expected_functions
        {
            let key = (function_name.to_string(), arity);
            let source_signature = source_interface
                .functions
                .get(&key)
                .unwrap_or_else(|| panic!("missing source signature {module_name}.{key:?}"));
            let summary_signature = summary_interface
                .functions
                .get(&key)
                .unwrap_or_else(|| panic!("missing summary signature {module_name}.{key:?}"));

            assert_eq!(source_signature.return_type, return_type);
            assert_eq!(summary_signature.return_type, return_type);
            assert_eq!(source_signature.params[0].name, receiver_name);
            assert_eq!(summary_signature.params[0].name, receiver_name);
            assert_eq!(source_signature.params[0].annotation, receiver_type);
            assert_eq!(summary_signature.params[0].annotation, receiver_type);
            assert!(source_signature.receiver_method);
            assert!(summary_signature.receiver_method);
            assert_eq!(source_signature.receiver_mutable, mutable);
            assert_eq!(summary_signature.receiver_mutable, mutable);
        }
    }
}

/// Verifies release iterator/iterable contracts produce stable interfaces.
///
/// Inputs:
/// - Release interface contracts for `std.collections.Iterator` and
///   `std.collections.Iterable`.
/// - Matching release `.typi` summaries.
///
/// Output:
/// - Test passes when source-contract extraction and summary parsing expose
///   the same key function and trait method signatures.
///
/// Transformation:
/// - Converts release interface syntax into HIR module interfaces and
///   compares those interfaces with the bodyless summaries planned for
///   later compiler phases.
#[test]
pub(super) fn hir_extracts_release_traversal_contracts_as_interfaces() {
    let iterator_source = parse_module_as_syntax_output(include_str!(
        "../../../../../../std/collections/Iterator.terl"
    ))
    .expect("parse iterator source contract");
    let iterator_summary = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.collections.Iterator.typi"
    ))
    .expect("parse iterator summary");
    let iterator_source_interface = syntax_module_output_to_interface(&iterator_source);
    let iterator_summary_interface = syntax_module_output_to_interface(&iterator_summary);

    assert_eq!(iterator_source_interface.module, "std.collections.Iterator");
    assert_eq!(
        iterator_summary_interface.module,
        "std.collections.Iterator"
    );
    assert_eq!(
        iterator_source_interface.functions[&("next".to_string(), 1)].return_type,
        "Option[Step[T]]"
    );
    assert_eq!(
        iterator_summary_interface.functions[&("next".to_string(), 1)].return_type,
        "Option[Step[T]]"
    );

    let iterable_source = parse_module_as_syntax_output(include_str!(
        "../../../../../../std/collections/Iterable.terl"
    ))
    .expect("parse iterable source contract");
    let iterable_summary = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.collections.Iterable.typi"
    ))
    .expect("parse iterable summary");
    let iterable_source_interface = syntax_module_output_to_interface(&iterable_source);
    let iterable_summary_interface = syntax_module_output_to_interface(&iterable_summary);

    assert_eq!(iterable_source_interface.module, "std.collections.Iterable");
    assert_eq!(
        iterable_summary_interface.module,
        "std.collections.Iterable"
    );
    assert_eq!(
        iterable_source_interface.traits["Iterable"].methods["iterator"].return_type,
        "std.collections.Iterator.Iterator[T]"
    );
    assert_eq!(
        iterable_summary_interface.traits["Iterable"].methods["iterator"].return_type,
        "std.collections.Iterator.Iterator[T]"
    );
}

/// Converts shared identifier names to lower snake case.
///
/// Inputs:
/// - Component/type names that exercise lowercase-to-uppercase,
///   digit-to-uppercase, and hyphen boundaries.
///
/// Output:
/// - Test assertions over normalized snake-case names.
///
/// Transformation:
/// - Covers the HIR naming helper reused by NativeBoundary naming, hover
///   rendering, and HTML component typechecking.
#[test]
pub(super) fn identifier_to_snake_handles_shared_component_names() {
    assert_eq!(identifier_to_snake("UserCard"), "user_card");
    assert_eq!(identifier_to_snake("Field2Value"), "field2_value");
    assert_eq!(identifier_to_snake("user-card"), "user_card");
    assert_eq!(identifier_to_snake("HTMLElement"), "html_element");
    assert_eq!(identifier_to_snake("URLValue"), "url_value");
    assert_eq!(identifier_to_snake("url"), "url");
}

/// Converts external source names into valid Terlan identifiers.
///
/// Inputs:
/// - JavaScript-like member names with camelCase, acronyms, symbols, numeric
///   starts, and keyword collisions.
///
/// Output:
/// - Test assertions over valid generated Terlan identifiers.
///
/// Transformation:
/// - Pins the shared naming helper used by generated JS/TypeScript bindings.
#[test]
pub(super) fn source_name_to_terlan_identifier_sanitizes_external_names() {
    assert_eq!(
        source_name_to_terlan_identifier("getElementById"),
        "get_element_by_id"
    );
    assert_eq!(source_name_to_terlan_identifier("URLValue"), "url_value");
    assert_eq!(source_name_to_terlan_identifier("type"), "type_");
    assert_eq!(
        source_name_to_terlan_identifier("2dContext"),
        "value_2d_context"
    );
    assert_eq!(source_name_to_terlan_identifier("$value"), "value");
}

/// Validates strict dependency-count handling for interface manifests.
#[test]
pub(super) fn interface_dependency_entries_require_declared_structured_rows() {
    let manifest = "module=std.demo\ndeps=2\nstd.core.Option=11\nstd.core.Ordering=22\n";
    assert_eq!(
        parse_interface_dependency_entries(manifest),
        Some(vec![
            ("std.core.Option".to_string(), 11),
            ("std.core.Ordering".to_string(), 22),
        ])
    );
    assert_eq!(
        parse_interface_dependency_entries("module=std.demo\ndeps=2\nstd.core.Option=11\n"),
        None
    );
    assert_eq!(
        parse_interface_dependency_entries("module=std.demo\ndeps=1\nmalformed\n"),
        None
    );
}
