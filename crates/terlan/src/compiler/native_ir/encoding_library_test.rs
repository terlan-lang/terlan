//! Portable decoder error policy executes source, independent of module name.

use super::source_constructor_test::check_sources;

#[test]
fn base64_error_policy_is_owned_by_terlan_source() {
    let provider = format!(
        "{}\n{}",
        include_str!("../../../../../std/encoding/Base64.terl"),
        r#"
pub check(): Bool ->
    let decoded = decode_result[String](Ok("source"));
    let malformed = decode_result[String](Err({false, "bad encoding"}));
    let utf8 = decode_result[Int](Err({true, "bad text"}));
    case {decoded, malformed, utf8} {
        {Ok("source"), Err(a), Err(b)} ->
            a.code == DecodeFailure and a.message == "bad encoding" and a.offset == 0
                and b.code == Utf8Failure and b.message == "bad text" and b.offset == 0;
        _ -> false
    }.
"#
    );
    for source in [
        provider.clone(),
        provider.replace("module std.encoding.Base64.", "module app.Codec."),
    ] {
        check_sources(&[&source]);
        check_sources(&[&source
            .replace("offset = 0", "offset = 17")
            .replace("offset == 0", "offset == 17")]);
    }
}

#[test]
fn retired_encoding_operations_are_not_runtime_fallbacks() {
    for name in [
        "decode",
        "decode_url",
        "decode_bytes",
        "decode_url_bytes",
        "unknown",
    ] {
        let operation = format!("std.encoding.base64.{name}");
        assert!(crate::std_native_packages::value_binding(&operation).is_none());
        assert!(crate::terlan_native_boundary::dispatch::operation_arity(&operation).is_none());
    }
}
