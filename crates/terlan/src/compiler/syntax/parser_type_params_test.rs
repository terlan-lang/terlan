//! Parser-boundary tests for canonical type-parameter syntax.

use crate::terlan_syntax::{
    parse_module, parse_tree::Decl, AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC,
    DUPLICATE_RECORD_TYPE_FIELD_DIAGNOSTIC, NEGATIVE_STRUCTURAL_IMPLICATION_DIAGNOSTIC,
};

/// Verifies negative structural evidence cannot enter generic constraints.
#[test]
fn rejects_negative_structural_implication() {
    let error = parse_module(
        r#"
module negative_structural_implication.

pub display[T => not {name: String}](value: T): String -> value.name.
"#,
    )
    .expect_err("negative structural implication must fail");

    assert_eq!(error.message, NEGATIVE_STRUCTURAL_IMPLICATION_DIAGNOSTIC);
}

/// Verifies the negative rejection does not affect positive evidence parsing.
#[test]
fn positive_structural_implication_remains_supported() {
    let module = parse_module(
        r#"
module positive_structural_implication.

pub display[T => {name: String}](value: T): String -> value.name.
"#,
    )
    .expect("positive structural implication should parse");

    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.generic_params, ["T => {name: String}"]);
}

/// Verifies duplicate evidence requirements fail before map normalization.
#[test]
fn rejects_duplicate_structural_implication_fields() {
    let error = parse_module(
        r#"
module duplicate_structural_implication.

pub display[T => {name: String, name: Int}](value: T): String -> value.name.
"#,
    )
    .expect_err("duplicate implication fields must fail");

    assert_eq!(error.message, AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC);
}

/// Verifies nested evidence shapes cannot normalize conflicting fields away.
#[test]
fn rejects_duplicate_nested_structural_implication_fields() {
    let error = parse_module(
        r#"
module duplicate_nested_structural_implication.

pub display[T => {profile: {name: String, name: Int}}](value: T): String -> value.profile.name.
"#,
    )
    .expect_err("duplicate nested implication fields must fail");

    assert_eq!(error.message, AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC);
}

/// Verifies nested shapes remain strict when wrapped by a generic type.
#[test]
fn rejects_duplicate_structural_implication_fields_inside_generic_types() {
    let error = parse_module(
        r#"
module duplicate_generic_nested_structural_implication.

pub display[T => {profiles: List[{name: String, name: Int}]}](value: T): T -> value.
"#,
    )
    .expect_err("generic-nested duplicate implication fields must fail");

    assert_eq!(error.message, AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC);
}

/// Verifies field uniqueness is scoped independently for nested shapes.
#[test]
fn nested_structural_implication_fields_use_independent_scopes() {
    let module = parse_module(
        r#"
module nested_structural_implication_fields.

pub display[T => {name: String, profile: {name: String}}](value: T): String -> value.name.
"#,
    )
    .expect("nested shapes may reuse an outer field name");

    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(
        function.generic_params,
        ["T => {name: String, profile: {name: String}}"]
    );
}

/// Verifies aliases cannot erase ambiguous record fields before implication use.
#[test]
fn rejects_duplicate_record_fields_in_implication_evidence_aliases() {
    let error = parse_module(
        r#"
module duplicate_implication_evidence_alias.

pub type Profile = {name: String, name: Int}.

pub display[T => {profile: Profile}](value: T): T -> value.
"#,
    )
    .expect_err("duplicate alias fields must fail before implication checking");

    assert_eq!(error.message, DUPLICATE_RECORD_TYPE_FIELD_DIAGNOSTIC);
}

/// Verifies ordinary record types retain independent nested field scopes.
#[test]
fn record_type_alias_fields_use_independent_nested_scopes() {
    let module = parse_module(
        r#"
module nested_record_type_alias_fields.

pub type Profile = {name: String, parent: {name: String}}.
"#,
    )
    .expect("nested record aliases may reuse an outer field name");

    let Decl::Type(alias) = &module.declarations[0] else {
        panic!("expected type alias declaration");
    };
    assert_eq!(
        alias.variants[0].text,
        "{name: String, parent: {name: String}}"
    );
}
