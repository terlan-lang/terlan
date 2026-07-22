use tower_lsp::lsp_types::{SymbolKind, Url};

use super::document::OpenDocuments;
use super::Backend;

/// Verifies editor Outline distinguishes negative and positive impl metadata.
#[test]
fn document_symbols_label_negative_trait_impls() {
    let symbols = Backend::document_symbols_for_text(
        "\
module symbols.NegativeTrait.

pub type SecretKey = String.
pub trait Log[T] { log(value: T): String. }.
pub impl not Log[SecretKey].
",
    );

    let children = symbols[0].children.as_ref().expect("module children");
    let negative = children
        .iter()
        .find(|symbol| symbol.name == "not Log for SecretKey")
        .expect("negative impl symbol");
    assert_eq!(negative.detail.as_deref(), Some("pub negative impl"));
    assert_eq!(negative.kind, SymbolKind::INTERFACE);
}

/// Verifies compiler-owned denial diagnostics reach editor snapshots unchanged.
#[test]
fn open_document_reports_denied_generic_trait_fallback() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/negative-trait-diagnostic.terl").expect("uri");
    store.open(
        uri.clone(),
        "\
module negative_trait_diagnostic.
pub opaque type SecretKey.
pub trait JsonEncode[T] { encode(value: T): String. }.
pub impl JsonEncode[T] for T {
    encode(value: T): String -> \"generic\".
}.
pub impl not JsonEncode[SecretKey].
pub encode_any[T](value: T)[JsonEncode[T]]: String -> JsonEncode.encode(value).
pub leak(value: SecretKey): String -> encode_any(value).
"
        .to_string(),
        1,
        "terlan".to_string(),
    );

    let document = store.snapshot(&uri).expect("document");
    assert!(document.parse_ok);
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document.type_diagnostics.iter().any(|diagnostic| {
        diagnostic.message
            == "at `encode_any` call site: trait bound `JsonEncode[SecretKey]` is explicitly denied"
    }));
}

/// Verifies comprehension lift diagnostics reach editor snapshots unchanged.
#[test]
fn open_document_reports_conflicting_comprehension_guard_containers() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/comprehension-guard-diagnostic.terl").expect("uri");
    store.open(
        uri.clone(),
        r#"
module comprehension_guard_diagnostic.

pub type First[T] = {Atom["first"], value: T}.
pub type Second[T] = {Atom["second"], value: T}.
pub trait GuardResult[R, F[_]] { into_guard(result: R): F[Bool]. }.
pub impl GuardResult[First[Bool], First] for First[Bool] {
    into_guard(result: First[Bool]): First[Bool] -> result.
}.
pub impl GuardResult[Second[Bool], Second] for Second[Bool] {
    into_guard(result: Second[Bool]): Second[Bool] -> result.
}.
first(value: Bool): First[Bool] -> First(value).
second(value: Bool): Second[Bool] -> Second(value).
pub values(items: List[Int]): Dynamic ->
    [value | value <- items, first(value > 0), second(value < 10)].
"#
        .to_string(),
        1,
        "terlan".to_string(),
    );

    let document = store.snapshot(&uri).expect("document");
    assert!(document.parse_ok);
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document
        .type_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("conflicting lift containers")));
}
