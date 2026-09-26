//! Tests for compiler-owned finite atom inventory.

use std::collections::BTreeSet;

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreCaseClause, CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreMapTypeField,
    CorePattern, CoreStructTypeField, CoreTupleTypeElem, CoreType,
};

use super::atom_inventory::{
    application_atom_identities, collect_expr, collect_pattern, collect_type,
    RUNTIME_BASE64_ERROR_ATOMS, RUNTIME_JSON_ERROR_ATOMS, RUNTIME_REGEX_ERROR_ATOMS,
    RUNTIME_URI_ERROR_ATOMS,
};

#[test]
fn random_error_atoms_follow_the_provider_or_import() {
    use super::atom_inventory::RUNTIME_RANDOM_ERROR_ATOMS;
    for (source, expected) in [
        ("module unrelated. pub value(): Int -> 1.", false),
        ("module std.random.Random. pub value(): Int -> 1.", true),
        (
            "module consumer. import std.random.Random. pub value(): Int -> 1.",
            true,
        ),
    ] {
        let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source).unwrap();
        let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
        let resolved =
            crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &interfaces)
                .module;
        let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
        let atoms = application_atom_identities(&[&core]);
        for code in RUNTIME_RANDOM_ERROR_ATOMS {
            assert_eq!(
                atoms.iter().any(|atom| atom == code),
                expected,
                "{source}: {code}"
            );
        }
    }
    assert_eq!(RUNTIME_RANDOM_ERROR_ATOMS.len(), 5);
}

/// Decoder errors are admitted only when the image includes the Base64 contract.
#[test]
fn base64_error_atoms_follow_the_provider_or_import() {
    for (source, expected) in [
        ("module unrelated. pub value(): Int -> 1.", false),
        ("module std.encoding.Base64. pub value(): Int -> 1.", true),
        (
            "module consumer. import std.encoding.Base64. pub value(): Int -> 1.",
            true,
        ),
    ] {
        let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source)
            .expect("parse atom inventory source");
        let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
        let resolved =
            crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &interfaces)
                .module;
        let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
        let atoms = application_atom_identities(&[&core]);
        for code in RUNTIME_BASE64_ERROR_ATOMS {
            assert_eq!(
                atoms.iter().any(|atom| atom == code),
                expected,
                "{source}: {code}"
            );
        }
    }
    assert_eq!(
        RUNTIME_BASE64_ERROR_ATOMS,
        &["base64.decode", "base64.utf8"]
    );
}

/// URI parse failures use a finite code only admitted with the URI provider.
#[test]
fn uri_error_atoms_follow_the_provider_or_import() {
    for (source, expected) in [
        ("module unrelated. pub value(): Int -> 1.", false),
        ("module std.net.Uri. pub value(): Int -> 1.", true),
        (
            "module consumer. import std.net.Uri. pub value(): Int -> 1.",
            true,
        ),
    ] {
        let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source)
            .expect("parse URI atom inventory");
        let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&syntax);
        let resolved =
            crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&syntax, &interfaces)
                .module;
        let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
        let atoms = application_atom_identities(&[&core]);
        assert_eq!(atoms.iter().any(|atom| atom == "uri.parse"), expected);
    }
    assert_eq!(RUNTIME_URI_ERROR_ATOMS, &["uri.parse"]);
}

/// Proves NativeBoundary JSON error identities remain a finite canonical set.
#[test]
fn runtime_json_error_atoms_are_sorted_and_unique() {
    assert!(RUNTIME_JSON_ERROR_ATOMS
        .windows(2)
        .all(|pair| pair[0] < pair[1]));
    assert!(RUNTIME_JSON_ERROR_ATOMS.contains(&"json.parse"));
    assert!(RUNTIME_JSON_ERROR_ATOMS.contains(&"json.key_not_found"));
    assert!(RUNTIME_JSON_ERROR_ATOMS.contains(&"json.duplicate_field"));
    assert!(RUNTIME_JSON_ERROR_ATOMS.contains(&"json.invalid_page"));
    assert!(RUNTIME_JSON_ERROR_ATOMS.contains(&"json.row_width_mismatch"));
}

/// Proves the regex adapter exposes one finite compilation error identity.
#[test]
fn runtime_regex_error_atoms_are_closed() {
    assert_eq!(RUNTIME_REGEX_ERROR_ATOMS, &["regex.compile"]);
}

/// Proves recursive type inventory is canonical and omits compact scalar singletons.
#[test]
fn type_inventory_collects_nested_atom_literals_canonically() {
    let ty = CoreType::Arrow {
        params: vec![
            CoreType::AtomLiteral("ready".to_owned()),
            CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral("error".to_owned())),
                CoreTupleTypeElem::Field {
                    name: "state".to_owned(),
                    ty: CoreType::AtomLiteral("ready".to_owned()),
                },
            ]),
            CoreType::Struct {
                name: "State".to_owned(),
                fields: vec![CoreStructTypeField {
                    name: "value".to_owned(),
                    ty: CoreType::AtomLiteral("true".to_owned()),
                    is_private: false,
                }],
            },
        ],
        return_type: Box::new(CoreType::Map(vec![CoreMapTypeField {
            key: "status".to_owned(),
            operator: ":".to_owned(),
            value: CoreType::Union(vec![
                CoreType::AtomLiteral("done".to_owned()),
                CoreType::AtomLiteral("Unit".to_owned()),
            ]),
        }])),
    };
    let mut atoms = BTreeSet::new();
    collect_type(&ty, &mut atoms);
    assert_eq!(
        atoms.into_iter().collect::<Vec<_>>(),
        ["done", "error", "ready"]
    );
}

/// Proves expression and pattern inventory reaches branch-local atom identities.
#[test]
fn expression_inventory_collects_branch_patterns_guards_and_bodies() {
    let expr = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Atom("pending".to_owned())),
        clauses: vec![CoreCaseClause {
            pattern: CorePattern::Constructor {
                name: "Result".to_owned(),
                constructor_identity: None,
                args: vec![CorePattern::Atom("ready".to_owned())],
            },
            guard: Some(CoreExpr::Atom("true".to_owned())),
            body: CoreExpr::Cast {
                expr: Box::new(CoreExpr::Atom("complete".to_owned())),
                target_type: CoreType::AtomLiteral("complete".to_owned()),
            },
        }],
    };
    let mut atoms = BTreeSet::new();
    collect_expr(&expr, &mut atoms);
    collect_pattern(&CorePattern::Atom("error".to_owned()), &mut atoms);
    assert_eq!(
        atoms.into_iter().collect::<Vec<_>>(),
        ["complete", "error", "pending", "ready"]
    );
}

/// Proves structured Option matching inventories its compact singleton value.
#[test]
fn option_none_pattern_inventories_compiler_synthesized_atom() {
    let mut atoms = BTreeSet::new();
    collect_pattern(
        &CorePattern::Constructor {
            name: "None".to_owned(),
            constructor_identity: Some("std.core.Option.None".to_owned()),
            args: Vec::new(),
        },
        &mut atoms,
    );
    assert_eq!(atoms.into_iter().collect::<Vec<_>>(), ["none"]);
}

/// Proves compiler-synthesized memory storage atoms enter the image inventory.
#[test]
fn memory_layout_intrinsic_inventories_storage_atoms() {
    let expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::MemoryLayoutOf(CoreType::Int),
        args: Vec::new(),
        return_type: CoreType::Named("std.core.Memory.Layout".to_string()),
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(0, 1),
    });
    let mut atoms = BTreeSet::new();
    collect_expr(&expr, &mut atoms);
    assert_eq!(
        atoms.into_iter().collect::<Vec<_>>(),
        ["Inline", "Managed", "Opaque"]
    );
}
