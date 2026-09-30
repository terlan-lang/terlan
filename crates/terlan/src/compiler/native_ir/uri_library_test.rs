//! URI accessors follow source bodies regardless of the library namespace.

use super::source_constructor_test::check_sources;

#[test]
fn uri_library_private_fields_and_accessors_are_source_owned() {
    let provider = r#"
module std.net.Uri.
pub struct Uri { #path: String, #text: String }.
pub parse(text: String): Uri -> Uri { #path: "path:" + text, #text: text + "!" }.
pub (uri: Uri) path(): String -> uri.#path + "?".
pub (uri: Uri) to_string(): String -> uri.#text.
pub (uri: Uri) matched_text(): String ->
    case uri { Uri { #text: text, #path: _ } -> text }.
"#;
    let caller = r#"
module uri_source_authority.
import std.net.Uri.
import std.net.Uri.{path as read_path}.
pub check(): Bool ->
    let uri = Uri.parse("example");
    uri.path() == "path:example?" and read_path(uri) == "path:example?"
        and uri.to_string() == "example!" and uri.matched_text() == "example!".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.net.Uri", "app.Uri"),
        &provider.replace("std.net.Uri", "app.Uri"),
    ]);
}
