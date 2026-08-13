use super::*;

/// Verifies required structural field types are not name-only evidence.
#[test]
pub(super) fn syntax_output_rejects_wrong_structural_implication_field_type() {
    let diagnostics = check_syntax_output(
        r#"
module implication_wrong_field_type.

pub struct User {
    name: Int
}.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render(user: User): String -> display_name(user).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` requires field type Binary, found Int"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies `Dynamic` cannot satisfy structural evidence through wildcard
/// unification and reports the concrete unsupported evidence source.
#[test]
pub(super) fn syntax_output_rejects_dynamic_structural_implication_evidence() {
    let diagnostics = check_syntax_output(
        r#"
module implication_dynamic_evidence.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render(value: Dynamic): String -> display_name(value).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` requires closed structural evidence, found Dynamic"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies an open key/value map is not treated as closed field evidence.
#[test]
pub(super) fn syntax_output_rejects_open_map_structural_implication_evidence() {
    let diagnostics = check_syntax_output(
        r#"
module implication_open_map_evidence.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render(value: Map[String, Dynamic]): String -> display_name(value).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` requires closed structural evidence for Map[Binary, Dynamic], but the type has no visible struct shape"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies generic forwarding requires evidence in the caller's scope.
#[test]
pub(super) fn syntax_output_rejects_unproven_forwarded_structural_implication() {
    let diagnostics = check_syntax_output(
        r#"
module implication_unproven_forward.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render[T](value: T): String -> display_name(value).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .starts_with("unproven_implication: `display_name` requires")),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies an implication grants only the fields named by its shape.
#[test]
pub(super) fn syntax_output_rejects_field_outside_structural_implication_scope() {
    let diagnostics = check_syntax_output(
        r#"
module implication_field_scope.

pub age[T => {name: String}](value: T): Int -> value.age.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "implication_scope_error: structural evidence for T0 does not include field `age`"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies private fields cannot satisfy ordinary structural evidence.
#[test]
pub(super) fn syntax_output_rejects_private_structural_implication_field() {
    let diagnostics = check_syntax_output(
        r#"
module implication_private_field.

pub struct SecretUser {
    #name: String
}.

pub display_name[T => {name: String}](value: T): String -> value.name.

pub render(user: SecretUser): String -> display_name(user).
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` cannot use private field `name` as structural evidence for SecretUser"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies public implication evidence and public struct fields survive an
/// interface boundary.
#[test]
pub(super) fn syntax_output_accepts_imported_structural_implication() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_consumer.

import implication_provider.{User, display_name}.

pub render(user: User): String -> display_name(user).
"#,
        r#"
module implication_provider.

pub struct User {
    name: String
}.

pub display_name[T => {name: String}](value: T): String.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies imported generic struct schemes preserve concrete field projection.
#[test]
pub(super) fn syntax_output_accepts_imported_generic_struct_structural_implication() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_generic_struct_consumer.

import implication_generic_struct_provider.{Page, Profile}.

pub render(page: Page[Profile]): String -> page.model.title.
"#,
        r#"
module implication_generic_struct_provider.

pub struct Profile {
    title: String
}.

pub struct Page[T => {title: String}] {
    model: T
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies imported generic projection retains incompatible concrete evidence.
#[test]
pub(super) fn syntax_output_rejects_unproven_imported_generic_struct_projection() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_generic_struct_projection_rejected.

import implication_generic_struct_provider.{Page}.

pub struct Account {
    id: Int
}.

pub display_title[T => {title: String}](value: T): String -> value.title.

pub render(page: Page[Account]): String -> display_title(page.model).
"#,
        r#"
module implication_generic_struct_provider.

pub struct Page[T => {title: String}] {
    model: T
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_title` requires field `title` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies public receiver-method implications survive interface rendering
/// and are validated against imported receiver shapes in consumers.
#[test]
pub(super) fn syntax_output_accepts_imported_receiver_method_structural_implication() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_receiver_consumer.

import implication_receiver_provider.{Presenter, User}.

pub render(presenter: Presenter, user: User): String ->
    presenter.display_name(user).
"#,
        r#"
module implication_receiver_provider.

pub struct Presenter {
    prefix: String
}.

pub struct User {
    name: String
}.

pub (presenter: Presenter) display_name[T => {name: String}](value: T): String.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies imported receiver-method implications remain mandatory when the
/// argument type is declared by the consuming module.
#[test]
pub(super) fn syntax_output_rejects_unproven_imported_receiver_method_structural_implication() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module implication_receiver_consumer_rejected.

import implication_receiver_provider.{Presenter}.

pub struct Account {
    id: Int
}.

pub render(presenter: Presenter, account: Account): String ->
    presenter.display_name(account).
"#,
        r#"
module implication_receiver_provider.

pub struct Presenter {
    prefix: String
}.

pub (presenter: Presenter) display_name[T => {name: String}](value: T): String.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
                == "unproven_implication: `display_name` requires field `name` on Account"
        }),
        "diagnostics: {diagnostics:?}"
    );
}
