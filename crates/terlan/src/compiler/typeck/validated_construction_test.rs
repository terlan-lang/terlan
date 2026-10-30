use super::test_support::{check_syntax_output, check_syntax_output_with_std_interfaces};

const OPTION_FIXTURE: &str = "std/core/Option.terl";
const RESULT_FIXTURE: &str = "std/core/Result.terl";

#[test]
fn validated_struct_rejects_public_bare_constructor() {
    let diagnostics = check_syntax_output(
        r#"
module checked.Profile.

@validated
pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Profile -> Profile { name: name }
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "public constructor `Profile` exposes validated type `Profile` without a checked result"
        )
        }),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn validated_struct_accepts_option_constructor() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        r#"
module checked.OptionalProfile.

import std.core.Option.{Option, Some}.

@validated
pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Option[Profile] -> Some(Profile { name: name })
}.
"#,
        OPTION_FIXTURE,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn validated_struct_accepts_result_constructor() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        r#"
module checked.ResultProfile.

import std.core.Result.{Result, Ok}.

@validated
pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Result[Profile, String] -> Ok(Profile { name: name })
}.
"#,
        RESULT_FIXTURE,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn validated_struct_accepts_explicit_trusted_constructor() {
    let diagnostics = check_syntax_output(
        r#"
module checked.TrustedProfile.

@validated
pub struct Profile {
    name: String
}.

@trusted
pub constructor Profile {
    (name: String): Profile -> Profile { name: name }
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn unannotated_struct_keeps_existing_constructor_contract() {
    let diagnostics = check_syntax_output(
        r#"
module checked.LegacyProfile.

pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Profile -> Profile { name: name }
}.
"#,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn validated_struct_rejects_lookalike_option_type() {
    let diagnostics = check_syntax_output(
        r#"
module checked.LookalikeOption.

pub type Option[T] = T.

@validated
pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Option[Profile] -> Profile { name: name }
}.
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "public constructor `Profile` exposes validated type `Profile` without a checked result"
        )
        }),
        "diagnostics: {diagnostics:?}"
    );
}
