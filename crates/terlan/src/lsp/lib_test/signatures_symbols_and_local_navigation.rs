use super::support::*;

/// Verifies signature parameter labels preserve full syntax-output metadata.
///
/// Inputs:
/// - A synthetic syntax-output parameter with mutability, pattern text, a type,
///   and a default value.
///
/// Output:
/// - Test success when the LSP label renderer keeps the same user-facing
///   parameter spelling editors should display.
///
/// Transformation:
/// - Calls the same formatter used by signature help without requiring a
///   parseable source fixture for syntax that is only present in summaries.
#[test]
pub(super) fn signature_parameter_label_preserves_mutability_patterns_and_defaults() {
    let param = SyntaxParamOutput {
        name: "user".to_string(),
        annotation: SyntaxTypeOutput {
            text: "User".to_string(),
            span: EbnfSourceSpan::default(),
        },
        is_mutable: true,
        has_default: true,
        default: None,
        default_text: Some("Guest".to_string()),
        pattern_text: Some("{name, family_name}".to_string()),
        span: EbnfSourceSpan::default(),
    };

    assert_eq!(
        Backend::signature_parameter_label(&param),
        "mut {name, family_name}: User = Guest"
    );
}

/// Verifies protocol-level inlay hints for simple inferred let bindings.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A Terlan source document with a simple integer literal binding.
///
/// Output:
/// - Test success when `textDocument/inlayHint` returns a deterministic type
///   hint at the binding-name boundary.
///
/// Transformation:
/// - Starts the real LSP service, opens a source document, drains diagnostics,
///   and requests inlay hints for the visible editor range.
#[tokio::test]
async fn inlay_hint_request_returns_literal_binding_type_hint() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-inlay-hints-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

pub lookup(user_id: Int, fallback: String): String.

pub struct RemoteRecord {
    id: Int
}.

pub (record: RemoteRecord) remote_label(label: String, suffix: String = \"!\"): String.
",
    )?;
    fs::create_dir_all(temp_dir.join("std/summaries"))?;
    fs::write(
        temp_dir.join("std/summaries/pkg.generated.Math.typi"),
        "\
module pkg.generated.Math.

pub generated_add(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("inlays.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let source = "\
module inlays.

import provider.{lookup}.
import provider.{RemoteRecord}.
import pkg.generated.Math.{generated_add}.

pub add(left: Int, right: Int = 0): Int ->
    left + right.

pub struct User {
    name: String
}.

pub (user: User) rename(label: String, suffix: String = \"!\"): String ->
    label + suffix.

pub caller(): Int ->
    let count = 1;
        let total = 2;
    add(1, 2) + count + total.

pub receiver_caller(user: User): String ->
    user.rename(\"Ada\", \"!\").

pub remote(): String ->
    lookup(7, \"missing\").

pub generated_remote(): Int ->
    generated_add(3, 4).

pub remote_receiver(record: RemoteRecord): String ->
    record.remote_label(\"Ada\", \"!\").

pub defaulted(): Int ->
    add(1).
";

    let (mut client_to_server, server_stdin) = duplex(4096);
    let (server_stdout, mut client_stdout) = duplex(4096);

    let server_task = tokio::spawn(async move {
        let (service, socket) = LspService::new(Backend::new);
        Server::new(server_stdin, server_stdout, socket)
            .serve(service)
            .await;
    });

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{}}}"#,
        )
        .await?;
    let _ = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "initialize response timeout"))??;

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )
    .await?;
    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": "terlan",
                    "version": 1,
                    "text": source
                }
            }
        })
        .to_string(),
    )
    .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, uri.as_str(), 1);

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/inlayHint",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 36, "character": 0 }
                }
            }
        })
        .to_string(),
    )
    .await?;
    let inlay_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "inlay response timeout"))??;
    assert!(inlay_response.contains(r#""id":2"#));
    assert!(inlay_response.contains(r#""position":{"character":13,"line":17}"#));
    assert!(inlay_response.contains(r#""label":": Int""#));
    assert!(inlay_response.contains(r#""kind":1"#));
    assert!(inlay_response.contains(r#""position":{"character":17,"line":18}"#));
    assert!(inlay_response.contains(r#""position":{"character":8,"line":19}"#));
    assert!(inlay_response.contains(r#""label":"left:""#));
    assert!(inlay_response.contains(r#""position":{"character":11,"line":19}"#));
    assert!(inlay_response.contains(r#""label":"right:""#));
    assert!(inlay_response.contains(r#""position":{"character":16,"line":22}"#));
    assert!(inlay_response.contains(r#""label":"label:""#));
    assert!(inlay_response.contains(r#""position":{"character":23,"line":22}"#));
    assert!(inlay_response.contains(r#""label":"suffix:""#));
    assert!(inlay_response.contains(r#""position":{"character":11,"line":25}"#));
    assert!(inlay_response.contains(r#""label":"user_id:""#));
    assert!(inlay_response.contains(r#""position":{"character":14,"line":25}"#));
    assert!(inlay_response.contains(r#""label":"fallback:""#));
    assert!(inlay_response.contains("Parameter `user_id` for `provider.lookup`."));
    assert!(inlay_response.contains(r#""position":{"character":18,"line":28}"#));
    assert!(inlay_response.contains(r#""position":{"character":21,"line":28}"#));
    assert!(inlay_response.contains("Parameter `left` for `pkg.generated.Math.generated_add`."));
    assert!(inlay_response.contains(r#""position":{"character":24,"line":31}"#));
    assert!(inlay_response.contains(r#""position":{"character":31,"line":31}"#));
    assert!(inlay_response.contains(r#""kind":2"#));
    assert!(inlay_response.contains(r#""label":"right = 0""#));
    assert!(inlay_response.contains("Defaulted parameter for `add`."));

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"#,
    )
    .await?;
    let _ = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "shutdown response timeout"))??;
    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    )
    .await?;
    drop(client_to_server);
    server_task.abort();
    let _ = server_task.await;
    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

/// Verifies the editor Outline sees the full source declaration tree.
///
/// Inputs:
/// - A parseable module containing a trait, struct fields, type alias,
///   constructor, public/private functions, receiver method, and explicit impl.
///
/// Output:
/// - Test success when LSP document symbols expose those declarations with
///   useful kinds, visibility details, and nested child symbols.
///
/// Transformation:
/// - Exercises compiler-backed syntax output through the LSP projection layer
///   without starting a JSON-RPC transport.
#[test]
pub(super) fn document_symbols_include_representative_outline_tree() {
    let symbols = Backend::document_symbols_for_text(
        "\
module symbols.Full.

pub trait Show[T] {
  show(value: T): String.
}.

pub struct User {
  id: Int,
  name: String
}.

pub type UserId = Int.

pub constructor User {
  (id: Int, name: String): User ->
    make_user(id, name)
}.

pub make_user(id: Int, name: String): User ->
  User(id = id, name = name).

helper(value: Int): Int ->
  value.

pub (user: User) display(): String ->
  user.name.

pub impl Show[User] for User {
  show(value: User): String ->
    value.name.
}.
",
    );

    assert_eq!(symbols.len(), 1);
    let module = &symbols[0];
    assert_eq!(module.name, "symbols.Full");
    assert_eq!(module.kind, SymbolKind::MODULE);

    let children = module.children.as_ref().expect("module children");
    let summary = children
        .iter()
        .map(|symbol| {
            (
                symbol.name.as_str(),
                symbol.detail.as_deref(),
                symbol.kind,
                symbol.children.as_ref().map(Vec::len).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![
            ("Show", Some("pub trait"), SymbolKind::INTERFACE, 1),
            ("User", Some("pub struct"), SymbolKind::STRUCT, 2),
            ("UserId", Some("pub type"), SymbolKind::STRUCT, 0),
            ("User", Some("pub constructor"), SymbolKind::CONSTRUCTOR, 0),
            ("make_user", Some("pub function"), SymbolKind::FUNCTION, 0),
            ("helper", Some("function"), SymbolKind::FUNCTION, 0),
            (
                "display",
                Some("pub receiver method"),
                SymbolKind::METHOD,
                0
            ),
            (
                "Show[User] for User",
                Some("pub impl"),
                SymbolKind::INTERFACE,
                1
            ),
        ]
    );

    let trait_children = children[0].children.as_ref().expect("trait methods");
    assert_eq!(trait_children.len(), 1);
    assert_eq!(trait_children[0].name, "show");
    assert_eq!(trait_children[0].detail.as_deref(), Some("trait method"));
    assert_eq!(trait_children[0].kind, SymbolKind::METHOD);

    let struct_children = children[1].children.as_ref().expect("struct fields");
    assert_eq!(
        struct_children
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.detail.as_deref(), symbol.kind))
            .collect::<Vec<_>>(),
        vec![
            ("id", Some("field"), SymbolKind::FIELD),
            ("name", Some("field"), SymbolKind::FIELD),
        ]
    );

    let impl_children = children[7].children.as_ref().expect("impl methods");
    assert_eq!(impl_children.len(), 1);
    assert_eq!(impl_children[0].name, "show");
    assert_eq!(impl_children[0].detail.as_deref(), Some("impl method"));
    assert_eq!(impl_children[0].kind, SymbolKind::METHOD);
}

#[test]
pub(super) fn document_symbols_return_empty_for_parse_errors() {
    let symbols = Backend::document_symbols_for_text("module broken");

    assert!(symbols.is_empty());
}

/// Verifies same-document go-to-definition resolves declaration symbols.
///
/// Inputs:
/// - An open document containing one function declaration and one call.
///
/// Output:
/// - Test passes when the cursor on the call target resolves to the function
///   declaration's selection range.
///
/// Transformation:
/// - Exercises the LSP definition helper without starting a JSON-RPC transport,
///   keeping the first definition slice focused on compiler-backed same-file
///   symbols.
#[test]
pub(super) fn definition_locations_resolve_same_document_function() {
    let uri = Url::parse("file:///tmp/definitions.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module definitions.

pub target(): Int ->
  1.

pub caller(): Int ->
  target().
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(6, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 10));
}

/// Verifies definition navigation survives formatter-induced span shifts.
///
/// Inputs:
/// - Compact source that the formatter expands into multiline function bodies.
///
/// Output:
/// - Test passes when go-to-definition on the formatted call resolves to the
///   formatted declaration selection range.
///
/// Transformation:
/// - Formats the source first and derives both cursor and expected target
///   positions from the formatted editor buffer.
#[test]
pub(super) fn definition_locations_use_formatter_shifted_function_spans() -> std_io::Result<()> {
    let formatted = format_source_module(
        "\
module formatter_definition_shift.

pub target(): Int -> 1.
pub caller(): Int -> target().
",
    )
    .map_err(|err| std_io::Error::new(ErrorKind::InvalidData, err.message))?;
    let declaration_start = formatted
        .find("target():")
        .unwrap_or_else(|| panic!("formatted source missing target declaration:\n{formatted}"));
    let call_start = formatted
        .rfind("target().")
        .unwrap_or_else(|| panic!("formatted source missing target call:\n{formatted}"));
    let expected_range = OpenDocument::range_from_span(
        &formatted,
        &Span::new(declaration_start, declaration_start + "target".len()),
    );
    let call_position =
        OpenDocument::range_from_span(&formatted, &Span::new(call_start, call_start + 1)).start;
    let uri = Url::parse("file:///tmp/formatter-definition-shift.terl")
        .map_err(|err| std_io::Error::new(ErrorKind::InvalidInput, err))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: formatted,
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations = Backend::definition_locations_for_position(&uri, &document, call_position);

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range, expected_range);
    Ok(())
}

/// Verifies go-to-definition resolves type references in annotations.
///
/// Inputs:
/// - An open document containing a type alias and a function annotation that
///   references it.
///
/// Output:
/// - Test passes when the cursor on the annotation type resolves to the type
///   declaration selection range.
///
/// Transformation:
/// - Locks the editor shift-click/Ctrl-click path for type names to the same
///   compiler-backed definition provider used for function calls.
#[test]
pub(super) fn definition_locations_resolve_same_document_type_annotation() {
    let uri = Url::parse("file:///tmp/type_definitions.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module type_definitions.

pub type Match =
  Int.

pub score(value: Match): Match ->
  value.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 18));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range.start, Position::new(2, 9));
    assert_eq!(locations[0].range.end, Position::new(2, 14));
}

/// Verifies receiver-call definitions prefer explicit impl methods.
///
/// Inputs:
/// - An open document containing a trait method and a matching explicit impl.
/// - A receiver-call expression using the implemented method name.
///
/// Output:
/// - Test passes when the cursor on `value.show()` resolves to the impl method
///   selection range instead of the abstract trait requirement.
///
/// Transformation:
/// - Locks definition navigation to executable receiver-method targets when
///   the source position is a dotted receiver member reference.
#[test]
pub(super) fn definition_locations_resolve_same_document_impl_method_reference() {
    let uri = Url::parse("file:///tmp/impl_method_definitions.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module impl_method_definitions.

pub trait Show[T] {
  show(value: T): String.
}.

pub struct User {
  name: String
}.

pub impl Show[User] for User {
  show(value: User): String ->
    value.name.
}.

pub caller(value: User): String ->
  value.show().
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(16, 9));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range.start, Position::new(11, 2));
    assert_eq!(locations[0].range.end, Position::new(11, 6));
}

/// Verifies field definitions remain navigable from field access.
///
/// Inputs:
/// - An open document containing a struct field and a dotted field access.
///
/// Output:
/// - Test passes when the cursor on `user.name` resolves to the struct field
///   declaration selection range.
///
/// Transformation:
/// - Locks field navigation to compiler-backed document symbols while allowing
///   receiver-method lookup to decline non-method dotted members.
#[test]
pub(super) fn definition_locations_resolve_same_document_field_reference() {
    let uri = Url::parse("file:///tmp/field_definitions.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module field_definitions.

pub struct User {
  id: Int,
  name: String
}.

pub display(user: User): String ->
  user.name.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(8, 8));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range.start, Position::new(4, 2));
    assert_eq!(locations[0].range.end, Position::new(4, 6));
}

/// Verifies wildcard selected imports resolve public provider definitions.
///
/// Inputs:
/// - A sibling provider `.terli` with public function `to_string/1`.
/// - A consumer document importing `provider.{*}` and calling `to_string`.
///
/// Output:
/// - Test passes when the cursor on the wildcard-imported function resolves to
///   the provider interface declaration range.
///
/// Transformation:
/// - Keeps wildcard selected imports tied to provider-summary visibility while
///   allowing editor navigation to use the actual identifier under the cursor.
#[test]
pub(super) fn definition_locations_resolve_wildcard_selected_imported_function(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-wildcard-imported-function-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub to_string(value: Int): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import provider.{*}.

pub caller(): String ->
  to_string(1).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 4));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 13));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies selected import aliases resolve to provider definitions.
///
/// Inputs:
/// - A sibling provider `.terli` with public function `to_string/1`.
/// - A consumer document importing `provider.{to_string as show}` and calling
///   `show`.
///
/// Output:
/// - Test passes when the cursor on the local alias resolves to the original
///   provider declaration range.
///
/// Transformation:
/// - Preserves alias ergonomics while keeping editor navigation anchored to the
///   public provider symbol rather than the alias site.
#[test]
pub(super) fn definition_locations_resolve_selected_import_alias_to_provider() -> std_io::Result<()>
{
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-aliased-imported-function-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub to_string(value: Int): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import provider.{to_string as show}.

pub caller(): String ->
  show(1).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 13));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies ambiguous imported symbols do not pick an arbitrary provider.
///
/// Inputs:
/// - Two sibling provider summaries both exporting public `value/0`.
/// - A consumer document selected-importing both `value` symbols and calling
///   `value`.
///
/// Output:
/// - Test passes when go-to-definition returns no provider location.
///
/// Transformation:
/// - Prevents editor navigation from hiding import ambiguity by returning the
///   first provider path discovered in source order.
#[test]
pub(super) fn definition_locations_reject_ambiguous_imported_symbol() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-ambiguous-import-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("left.terli"),
        "\
module left.

pub value(): Int.
",
    )?;
    fs::write(
        temp_dir.join("right.terli"),
        "\
module right.

pub value(): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import left.{value}.
import right.{value}.

pub caller(): Int ->
  value().
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(6, 3));

    assert!(
        locations.is_empty(),
        "ambiguous import locations: {locations:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
