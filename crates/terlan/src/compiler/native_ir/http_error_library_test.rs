//! HTTP error behavior follows library source, not compiler-owned projections.

use super::source_constructor_test::check_sources;

#[test]
fn http_error_library_uses_inherited_source_fields() {
    check_sources(&[
        r#"
module http_error_fields.
import std.http.Error.
import std.http.Error.{new as make, code as error_code, message as error_message, status as error_status}.
pub type InvalidBody.
pub check(): Bool ->
    let value = make(InvalidBody, "", 400);
    let other = Error.new(InvalidBody, "invalid body", 503);
    error_code(value) == InvalidBody and error_message(value) == ""
        and error_status(value) == 400
        and other.code() == InvalidBody and other.message() == "invalid body"
        and other.status() == 503 and other.status == 503.
"#,
        include_str!("../../../../../std/http/Error.terl"),
        include_str!("../../../../../std/core/Error.terl"),
    ]);
}

#[test]
fn http_error_library_can_change_body_layout_and_namespace() {
    let provider = r#"
module std.http.Error.
pub struct HttpError { status: Int, message: String, code: Atom }.
pub new(code: Atom, message: String, status: Int): HttpError ->
    HttpError(status = status + 1, message = message + "!", code = code).
pub (error: HttpError) code(): Atom -> error.code.
pub (error: HttpError) message(): String -> "source:" + error.message.
pub (error: HttpError) status(): Int -> error.status + 2.
"#;
    let caller = r#"
module http_error_authority.
import std.http.Error.
import std.http.Error.{new as make, status as read_status}.
pub type InvalidBody.
pub check(): Bool ->
    let value = Error.new(InvalidBody, "bad", 400);
    value.code() == InvalidBody and value.message() == "source:bad!"
        and read_status(value) == 403 and value.status() == 403
        and value.status == 401 and make(InvalidBody, "", 0).message() == "source:!".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Error", "app.Error"),
        &provider.replace("std.http.Error", "app.Error"),
    ]);
}

#[test]
fn http_error_calls_reject_wrong_arity_and_field_types() {
    for expression in [
        "Error.new(InvalidBody)",
        "Error.new(InvalidBody, 42, 400)",
        "Error.new(InvalidBody, \"bad\", true)",
    ] {
        let source = format!(
            "module invalid_http_error. import std.http.Error. pub type InvalidBody. \
             pub check(): Bool -> let value = {expression}; value.status() == 400."
        );
        assert!(
            crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
                "invalid_http_error.terl",
                &source,
                crate::DiagnosticFormat::default(),
                None,
                crate::validation::native_policy::NativePolicy::default(),
                crate::validation::target_profile::TargetProfile::Vm,
            )
            .is_err(),
            "{expression} must be rejected by ordinary type checking"
        );
    }
}
