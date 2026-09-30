//! Real source modules execute through copied-value bindings, not worker shims.

use super::source_test_support::assert_source_checks;

#[test]
fn compiled_encoding_preserves_text_bytes_and_source_owned_errors() {
    assert_source_checks(
        "encoding_source",
        r#"
module encoding_source.
import std.encoding.{Base64, Md5}.
import std.core.Result.{Ok, Err}.
import std.vm.Bytes.
type DecodeFailure = Atom["base64.decode"].
type Utf8Failure = Atom["base64.utf8"].

pub text(): Bool ->
    case {Base64.decode(Base64.encode("hello")), Base64.decode_url(Base64.encode_url(""))} {
        {Ok("hello"), Ok("")} -> true;
        _ -> false
    }.

pub bytes(): Bool ->
    let payload = Bytes.from_list([0, 127, 128, 255]);
    case {Base64.decode_bytes(Base64.encode_bytes(payload)), Base64.decode_url_bytes(Base64.encode_url_bytes(payload))} {
        {Ok(first), Ok(second)} -> first == payload and second == payload;
        _ -> false
    }.

pub errors(): Bool ->
    case {Base64.decode("!"), Base64.decode("//4="), Base64.decode_bytes("//4="), Base64.decode_url("__4=")} {
        {Err(a), Err(b), Ok(bytes), Err(c)} ->
            a.code == DecodeFailure and a.offset == 0 and a.message != ""
                and b.code == Utf8Failure and b.offset == 0 and b.message != ""
                and c.code == Utf8Failure and bytes == Bytes.from_list([255, 254]);
        _ -> false
    }.

pub digest(): Bool -> Md5.digest("abc") == "900150983cd24fb0d6963f7d28e17f72".
"#,
        &["text", "bytes", "errors", "digest"],
    );
}
