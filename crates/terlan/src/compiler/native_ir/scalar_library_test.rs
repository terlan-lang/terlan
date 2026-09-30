//! Scalar library bodies, including receiver callbacks, are source-owned.

use super::source_constructor_test::check_sources;

#[test]
fn sha256_library_name_does_not_replace_source_body() {
    check_sources(&[
        r#"
module hash_source_authority.
import std.crypto.Hash.
import std.crypto.Hash.{sha256 as digest}.
pub check(): Bool -> Hash.sha256("abc") == "source:abc" and digest("") == "source:".
"#,
        r#"module std.crypto.Hash. pub sha256(text: String): String -> "source:" + text."#,
    ]);
}

#[test]
fn float_constants_use_their_source_bodies() {
    let caller = r#"
module float_source_authority.
import std.core.Float.
import std.core.Float.{pi as circle_pi, tau as circle_tau}.
pub check(): Bool -> Float.pi() == circle_pi() and Float.tau() == circle_tau()
    and circle_pi() == 3.0 and circle_tau() == 6.0.
"#;
    check_sources(&[
        caller,
        "module std.core.Float. pub pi(): Float -> 3.0. pub tau(): Float -> 6.0.",
    ]);
    check_sources(&[
        r#"
module float_constants.
import std.core.Float.
pub check(): Bool -> Float.pi() == 3.141592653589793
    and Float.tau() == 6.283185307179586.
"#,
        include_str!("../../../../../std/core/Float.terl"),
    ]);
}

#[test]
fn string_std_tests_typecheck_with_implicit_prelude_methods() {
    let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "std/core/StringTest.terl",
        include_str!("../../../../../std/core/StringTest.terl"),
        crate::DiagnosticFormat::default(),
        None,
        crate::validation::native_policy::NativePolicy::default(),
        crate::validation::target_profile::TargetProfile::Vm,
    );
    assert!(compiled.is_ok(), "String's real test module must typecheck");
}

#[test]
fn string_library_names_do_not_replace_source_implementations() {
    check_sources(&[
        r#"
module string_provider_authority.
import std.core.String.
import std.core.String.{equal as same, compare as order, to_string as render,
    from_string as parse, is_empty as empty, append as join}.
import std.core.Option.{Some}.
apply[T, R](value: T, transform: (T) -> R): R -> transform(value).
pub check(): Bool ->
    same("a", "b") and not String.equal("a", "a") and "a".equal("b")
    and order("a", "b") == 42 and String.compare("b", "a") == 42
    and "a".compare("b") == 42
    and render("a") == "provider" and String.to_string("b") == "provider"
    and "a".to_string() == "provider"
    and parse("a") == Some("parsed") and String.from_string("b") == Some("parsed")
    and "a".from_string() == Some("parsed")
    and empty("not-empty") and String.is_empty("not-empty") and "not-empty".is_empty()
    and join("a", "b") == "ba" and String.append("a", "b") == "ba"
    and "a".append("b") == "ba"
    and apply("a", (text) -> text.append("b")) == "ba"
    and apply("a", (text) -> text.to_string()) == "provider"
    and apply("a", (text) -> text.from_string()) == Some("parsed")
    and apply("a", (text) -> text.is_empty())
    and apply("a", (text) -> text.equal("b"))
    and apply("a", (text) -> text.compare("b")) == 42
    and "a" != "b" and "a" < "b" and "a" + "b" == "ab".
"#,
        r#"
module std.core.String.
import std.core.Option.{Some}.
import type std.core.Option.
pub (left: String) equal(right: String): Bool -> left != right.
pub (left: String) compare(right: String): Int -> 42.
pub (value: String) to_string(): String -> "provider".
pub (value: String) from_string(): Option[String] -> Some("parsed").
pub (value: String) is_empty(): Bool -> value != "".
pub (left: String) append(right: String): String -> right + left.
"#,
        include_str!("../../../../../std/core/Option.terl"),
    ]);
}

#[test]
fn string_library_executes_unicode_and_empty_value_contracts() {
    check_sources(&[
        r#"
module string_library_contracts.
import type std.collections.List.
import std.core.Option.{Some}.
import std.core.Ordering.{Lt, Eq, Gt}.
apply[T, R](value: T, transform: (T) -> R): R -> transform(value).
pub check(): Bool ->
    "".is_empty() and not " ".is_empty() and not "\u0000".is_empty()
    and "".equal("") and "é".equal("é") and not "é".equal("é")
    and "".compare("a") == Lt and "a".compare("a") == Eq
    and "é".compare("z") == Gt
    and "".to_string() == "" and "\u0000é".to_string() == "\u0000é"
    and "".from_string() == Some("") and "\u0000é".from_string() == Some("\u0000é")
    and "".append("") == "" and "é".append("") == "é"
    and "".append("é") == "é" and "é".append("界") == "é界"
    and apply("é", (text) -> text.append("界")) == "é界"
    and apply("é", (text) -> text.to_string()) == "é"
    and apply("é", (text) -> text.from_string()) == Some("é")
    and apply("", (text) -> text.is_empty()).
"#,
        include_str!("../../../../../std/core/String.terl"),
        include_str!("../../../../../std/core/Option.terl"),
        include_str!("../../../../../std/core/Ordering.terl"),
    ]);
}
