
/// Verifies completion includes local and imported shape declarations.
///
/// Inputs:
/// - A source module with local type/struct/trait names, one raw shape,
///   constructor, function, active local parameters/bindings, receiver field
///   access, and imported provider type/struct/trait/shape/constructor/function
///   names.
/// - A temporary provider `.terli` carrying public type-level, shape,
///   constructor, and function metadata.
///
/// Output:
/// - Test success when completion items include type-level names, shape and
///   function names, constructor names, receiver fields, local values, details,
///   and Markdown documentation.
///
/// Transformation:
/// - Exercises compiler syntax output and generated-summary loading without a
///   JSON-RPC transport so completion stays tied to the same data as hover and
///   documentation.
#[test]
fn completion_items_include_local_and_imported_shapes_and_functions() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-shape-completion-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
//! Provider module docs.
module provider.

/// Provider ID alias.
pub type RemoteId = Int.

type HiddenId = Int.

/// Provider record.
pub struct RemoteRecord {
  id: Int,
  #secret: String
}.

/// Labels a provider record.
@pure
pub (record: RemoteRecord) remote_label(): String.

/// Provider display trait.
pub trait RemoteShow[T] {
  show(value: T): String.
}.

/// Matches a user asset path.
pub shape UserAsset(id, file) =
  \"users/${id: Int}/assets/${file}\".

/// Builds a remote user.
pub constructor RemoteUser {
  (id: Int): RemoteUser ->
    RemoteUser(id)
}.

/// Looks up a provider user.
@pure
pub lookup(id: Int): String.

/// Provider retry count.
pub const RETRY_COUNT: Int = 3.

hidden_lookup(id: Int): String.
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

import provider.{RemoteId, RemoteRecord, RemoteShow, UserAsset, RemoteUser, lookup, RETRY_COUNT}.

/// Local retry count.
const LOCAL_RETRY_COUNT: Int = 2.

/// Local ID alias.
pub type LocalId = Int.

/// Local record.
pub struct LocalRecord {
  id: Int
}.

/// Labels a local record.
@pure
pub (record: LocalRecord) local_label(): String ->
  \"local\".

/// Local display trait.
pub trait LocalShow[T] {
  show(value: T): String.
}.

pub impl LocalShow[LocalRecord] for LocalRecord {
  show(value: LocalRecord): String ->
    \"local\".
}.

/// Builds a local user.
pub constructor LocalUser {
  (name: String): LocalUser ->
    LocalUser(name)
}.

/// Matches a local asset path.
shape LocalAsset(id) =
  \"local/${id: Int}\".

/// Calls the local app.
@pure
pub caller(input: Int, record: LocalRecord, remote: RemoteRecord): Int ->
  let count = 1;
      let total = 2;
  record.id + remote.id + count + total + input.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let position_after = |needle: &str| {
        let offset = document
            .text
            .find(needle)
            .unwrap_or_else(|| panic!("missing completion fixture marker {needle:?}"))
            + needle.len();
        let prefix = &document.text[..offset];
        let line = prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32;
        let character = prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32;
        Position::new(line, character)
    };

    let items = Backend::completion_items_for_position(
        &uri,
        &document,
        position_after("let count = 1;\n  "),
    );
    let chained_items = Backend::completion_items_for_position(
        &uri,
        &document,
        position_after("      let total = 2;\n  "),
    );
    let local_field_items =
        Backend::completion_items_for_position(&uri, &document, position_after("  record."));
    let imported_field_items =
        Backend::completion_items_for_position(&uri, &document, position_after(" + remote."));
    let local_type = items
        .iter()
        .find(|item| item.label == "LocalId")
        .expect("local type completion");
    assert_eq!(local_type.kind, Some(CompletionItemKind::TYPE_PARAMETER));
    assert_eq!(local_type.detail.as_deref(), Some("type LocalId"));
    assert_eq!(local_type.insert_text.as_deref(), Some("LocalId"));
    assert!(completion_doc_text(local_type).contains("Local ID alias."));

    let local_struct = items
        .iter()
        .find(|item| item.label == "LocalRecord")
        .expect("local struct completion");
    assert_eq!(local_struct.kind, Some(CompletionItemKind::STRUCT));
    assert_eq!(local_struct.detail.as_deref(), Some("struct LocalRecord"));
    assert_eq!(local_struct.insert_text.as_deref(), Some("LocalRecord"));
    assert!(completion_doc_text(local_struct).contains("Local record."));

    let local_trait = items
        .iter()
        .find(|item| item.label == "LocalShow")
        .expect("local trait completion");
    assert_eq!(local_trait.kind, Some(CompletionItemKind::INTERFACE));
    assert_eq!(local_trait.detail.as_deref(), Some("trait LocalShow"));
    assert_eq!(local_trait.insert_text.as_deref(), Some("LocalShow"));
    assert!(completion_doc_text(local_trait).contains("Local display trait."));

    let local = items
        .iter()
        .find(|item| item.label == "LocalAsset")
        .expect("local shape completion");
    assert_eq!(local.kind, Some(CompletionItemKind::STRUCT));
    assert_eq!(local.detail.as_deref(), Some("shape"));
    assert!(completion_doc_text(local).contains("Matches a local asset path."));

    let local_constructor = items
        .iter()
        .find(|item| item.label == "LocalUser")
        .expect("local constructor completion");
    assert_eq!(
        local_constructor.kind,
        Some(CompletionItemKind::CONSTRUCTOR)
    );
    assert_eq!(
        local_constructor.detail.as_deref(),
        Some("constructor LocalUser/1 -> LocalUser")
    );
    assert_eq!(
        local_constructor.insert_text.as_deref(),
        Some("LocalUser()")
    );
    assert!(completion_doc_text(local_constructor).contains("Builds a local user."));

    let local_function = items
        .iter()
        .find(|item| item.label == "caller")
        .expect("local function completion");
    assert_eq!(local_function.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(
        local_function.detail.as_deref(),
        Some("pure function caller/3 -> Int")
    );
    assert_eq!(local_function.insert_text.as_deref(), Some("caller()"));
    assert!(completion_doc_text(local_function).contains("Calls the local app."));

    let local_constant = items
        .iter()
        .find(|item| item.label == "LOCAL_RETRY_COUNT")
        .expect("local constant completion");
    assert_eq!(local_constant.kind, Some(CompletionItemKind::CONSTANT));
    assert_eq!(
        local_constant.detail.as_deref(),
        Some("const LOCAL_RETRY_COUNT: Int")
    );
    assert!(completion_doc_text(local_constant).contains("Local retry count."));

    let imported_constant = items
        .iter()
        .find(|item| item.label == "RETRY_COUNT")
        .expect("imported constant completion");
    assert_eq!(imported_constant.kind, Some(CompletionItemKind::CONSTANT));
    assert_eq!(
        imported_constant.detail.as_deref(),
        Some("const provider.RETRY_COUNT: Int = 3")
    );
    assert!(completion_doc_text(imported_constant).contains("Provider retry count."));

    let parameter = items
        .iter()
        .find(|item| item.label == "input")
        .expect("parameter completion");
    assert_eq!(parameter.kind, Some(CompletionItemKind::VARIABLE));
    assert_eq!(parameter.detail.as_deref(), Some("parameter Int"));
    assert_eq!(parameter.insert_text.as_deref(), Some("input"));

    let local_binding = items
        .iter()
        .find(|item| item.label == "count")
        .expect("local let-binding completion");
    assert_eq!(local_binding.kind, Some(CompletionItemKind::VARIABLE));
    assert_eq!(local_binding.detail.as_deref(), Some("local Int"));
    assert_eq!(local_binding.insert_text.as_deref(), Some("count"));

    let chained_local_binding = chained_items
        .iter()
        .find(|item| item.label == "total")
        .expect("chained local let-binding completion");
    assert_eq!(
        chained_local_binding.kind,
        Some(CompletionItemKind::VARIABLE)
    );
    assert_eq!(chained_local_binding.detail.as_deref(), Some("local Int"));
    assert_eq!(chained_local_binding.insert_text.as_deref(), Some("total"));

    let local_field = local_field_items
        .iter()
        .find(|item| item.label == "id")
        .expect("local receiver field completion");
    assert_eq!(local_field.kind, Some(CompletionItemKind::FIELD));
    assert_eq!(
        local_field.detail.as_deref(),
        Some("field LocalRecord.id: Int")
    );
    assert_eq!(local_field.insert_text.as_deref(), Some("id"));

    let local_receiver_method = local_field_items
        .iter()
        .find(|item| item.label == "local_label")
        .expect("local receiver method completion");
    assert_eq!(local_receiver_method.kind, Some(CompletionItemKind::METHOD));
    assert_eq!(
        local_receiver_method.detail.as_deref(),
        Some("pure method LocalRecord.local_label/0 -> String")
    );
    assert_eq!(
        local_receiver_method.insert_text.as_deref(),
        Some("local_label()")
    );
    assert!(completion_doc_text(local_receiver_method).contains("Labels a local record."));

    let local_impl_method = local_field_items
        .iter()
        .find(|item| item.label == "show")
        .expect("local impl method completion");
    assert_eq!(local_impl_method.kind, Some(CompletionItemKind::METHOD));
    assert_eq!(
        local_impl_method.detail.as_deref(),
        Some("method LocalRecord.show/0 -> String")
    );
    assert_eq!(local_impl_method.insert_text.as_deref(), Some("show()"));

    let imported_field = imported_field_items
        .iter()
        .find(|item| item.label == "id")
        .expect("imported receiver field completion");
    assert_eq!(imported_field.kind, Some(CompletionItemKind::FIELD));
    assert_eq!(
        imported_field.detail.as_deref(),
        Some("field RemoteRecord.id: Int")
    );
    assert_eq!(imported_field.insert_text.as_deref(), Some("id"));
    assert!(
        imported_field_items
            .iter()
            .all(|item| item.label != "secret"),
        "private imported field should not be suggested"
    );

    let imported_receiver_method = imported_field_items
        .iter()
        .find(|item| item.label == "remote_label")
        .expect("imported receiver method completion");
    assert_eq!(
        imported_receiver_method.kind,
        Some(CompletionItemKind::METHOD)
    );
    assert_eq!(
        imported_receiver_method.detail.as_deref(),
        Some("pure method RemoteRecord.remote_label/0 -> String")
    );
    assert_eq!(
        imported_receiver_method.insert_text.as_deref(),
        Some("remote_label()")
    );
    assert!(completion_doc_text(imported_receiver_method).contains("Labels a provider record."));

    let imported_type = items
        .iter()
        .find(|item| item.label == "RemoteId")
        .expect("imported type completion");
    assert_eq!(imported_type.kind, Some(CompletionItemKind::TYPE_PARAMETER));
    assert_eq!(
        imported_type.detail.as_deref(),
        Some("type provider.RemoteId")
    );
    assert_eq!(imported_type.insert_text.as_deref(), Some("RemoteId"));
    assert!(completion_doc_text(imported_type).contains("Provider ID alias."));

    let imported_struct = items
        .iter()
        .find(|item| item.label == "RemoteRecord")
        .expect("imported struct completion");
    assert_eq!(imported_struct.kind, Some(CompletionItemKind::STRUCT));
    assert_eq!(
        imported_struct.detail.as_deref(),
        Some("struct provider.RemoteRecord")
    );
    assert_eq!(imported_struct.insert_text.as_deref(), Some("RemoteRecord"));
    assert!(completion_doc_text(imported_struct).contains("Provider record."));

    let imported_trait = items
        .iter()
        .find(|item| item.label == "RemoteShow")
        .expect("imported trait completion");
    assert_eq!(imported_trait.kind, Some(CompletionItemKind::INTERFACE));
    assert_eq!(
        imported_trait.detail.as_deref(),
        Some("trait provider.RemoteShow")
    );
    assert_eq!(imported_trait.insert_text.as_deref(), Some("RemoteShow"));
    assert!(completion_doc_text(imported_trait).contains("Provider display trait."));

    let imported = items
        .iter()
        .find(|item| item.label == "UserAsset")
        .expect("imported shape completion");
    assert_eq!(imported.kind, Some(CompletionItemKind::STRUCT));
    assert_eq!(imported.detail.as_deref(), Some("shape provider.UserAsset"));
    assert!(completion_doc_text(imported).contains("Matches a user asset path."));

    let imported_constructor = items
        .iter()
        .find(|item| item.label == "RemoteUser")
        .expect("imported constructor completion");
    assert_eq!(
        imported_constructor.kind,
        Some(CompletionItemKind::CONSTRUCTOR)
    );
    assert_eq!(
        imported_constructor.detail.as_deref(),
        Some("constructor provider.RemoteUser/1 -> RemoteUser")
    );
    assert_eq!(
        imported_constructor.insert_text.as_deref(),
        Some("RemoteUser()")
    );
    assert!(completion_doc_text(imported_constructor).contains("Builds a remote user."));

    let imported_function = items
        .iter()
        .find(|item| item.label == "lookup")
        .expect("imported function completion");
    assert_eq!(imported_function.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(
        imported_function.detail.as_deref(),
        Some("pure function provider.lookup/1 -> String")
    );
    assert_eq!(imported_function.insert_text.as_deref(), Some("lookup()"));
    assert!(completion_doc_text(imported_function).contains("Looks up a provider user."));

    for hidden_label in ["HiddenId", "hidden_lookup"] {
        let leaked = items
            .iter()
            .filter(|item| item.label == hidden_label)
            .map(|item| format!("{} {:?}", item.label, item.detail))
            .collect::<Vec<_>>();
        assert!(
            leaked.is_empty(),
            "private imported completion `{hidden_label}` should not be suggested: {leaked:?}"
        );
    }

    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies protocol-level signature help for local function calls.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A Terlan source document with a local function carrying typed/defaulted
///   parameters and a same-file call.
///
/// Output:
/// - Test success when `textDocument/signatureHelp` returns the local function
///   signature and marks the second parameter active after a comma.
///
/// Transformation:
/// - Starts the real LSP service, opens a source document, drains diagnostics,
///   and requests signature help at the call-site position used by editors.
#[tokio::test]
async fn signature_help_request_returns_local_function_signature() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-signature-help-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

pub struct RemoteRecord {
    id: Int
}.

/// Labels a provider record.
@pure
pub (record: RemoteRecord) remote_label(label: String, suffix: String = \"!\"): String.

/// Adds provider values.
@pure
pub remote_add[A](left: A, right: A): A.
",
    )?;
    fs::create_dir_all(temp_dir.join("std/summaries"))?;
    fs::write(
        temp_dir.join("std/summaries/pkg.generated.Math.typi"),
        "\
module pkg.generated.Math.

/// Adds generated provider values.
@pure
pub generated_add(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("signatures.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let source = "\
module signatures.

import provider.{RemoteRecord, remote_add}.
import pkg.generated.Math.{generated_add}.

@pure
pub add(left: Int, right: Int = 0): Int ->
    left + right.

@pure
pub identity[T](value: T): T ->
    value.

pub caller(): Int ->
    add(1, 2).

pub generic_caller(): Int ->
    identity(1).

pub imported_function_caller(): Int ->
    remote_add(1, 2).

pub generated_function_caller(): Int ->
    generated_add(1, 2).

pub struct User {
    name: String
}.

/// Renames a user.
@pure
pub (user: User) rename(label: String, suffix: String = \"!\"): String ->
    label + suffix.

pub receiver_caller(user: User): String ->
    user.rename(\"Ada\", \"!\").

pub imported_receiver_caller(remote: RemoteRecord): String ->
    remote.remote_label(\"Ada\", \"!\").
";
    let position_after = |needle: &str| {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("missing signature fixture marker {needle:?}"))
            + needle.len();
        let prefix = &source[..offset];
        let line = prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32;
        let character = prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32;
        Position::new(line, character)
    };
    let add_position = position_after("add(1,");
    let identity_position = position_after("identity(");
    let remote_add_position = position_after("remote_add(1,");
    let generated_add_position = position_after("generated_add(1,");
    let rename_position = position_after("user.rename(\"Ada\",");
    let remote_label_position = position_after("remote.remote_label(\"Ada\",");

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
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": add_position.line,
                    "character": add_position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let signature_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "signature response timeout"))??;
    assert!(signature_response.contains(r#""id":2"#));
    assert!(signature_response.contains(r#""label":"pure add(left: Int, right: Int = 0): Int""#));
    assert!(signature_response.contains(r#""label":"left: Int""#));
    assert!(signature_response.contains(r#""label":"right: Int = 0""#));
    assert!(signature_response.contains(r#""activeParameter":1"#));

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": identity_position.line,
                    "character": identity_position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let generic_signature_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "generic signature response timeout"))??;
    assert!(generic_signature_response.contains(r#""id":3"#));
    assert!(
        generic_signature_response.contains(r#""label":"pure identity[T](value: T): T""#),
        "{generic_signature_response}"
    );
    assert!(generic_signature_response.contains(r#""label":"value: T""#));
    assert!(generic_signature_response.contains(r#""activeParameter":0"#));

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": remote_add_position.line,
                    "character": remote_add_position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let imported_function_signature_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| {
        std_io::Error::new(
            ErrorKind::TimedOut,
            "imported function signature response timeout",
        )
    })??;
    assert!(imported_function_signature_response.contains(r#""id":4"#));
    assert!(
        imported_function_signature_response
            .contains(r#""label":"pure remote_add[A](left: A, right: A): A""#),
        "{imported_function_signature_response}"
    );
    assert!(imported_function_signature_response.contains(r#""label":"left: A""#));
    assert!(imported_function_signature_response.contains(r#""label":"right: A""#));
    assert!(imported_function_signature_response.contains(r#""activeParameter":1"#));
    assert!(imported_function_signature_response.contains("Adds provider values."));

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": generated_add_position.line,
                    "character": generated_add_position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let generated_function_signature_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| {
        std_io::Error::new(
            ErrorKind::TimedOut,
            "generated function signature response timeout",
        )
    })??;
    assert!(generated_function_signature_response.contains(r#""id":5"#));
    assert!(
        generated_function_signature_response
            .contains(r#""label":"pure generated_add(left: Int, right: Int): Int""#),
        "{generated_function_signature_response}"
    );
    assert!(generated_function_signature_response.contains(r#""label":"left: Int""#));
    assert!(generated_function_signature_response.contains(r#""label":"right: Int""#));
    assert!(generated_function_signature_response.contains(r#""activeParameter":1"#));
    assert!(generated_function_signature_response.contains("Adds generated provider values."));

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": rename_position.line,
                    "character": rename_position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let receiver_signature_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| {
        std_io::Error::new(ErrorKind::TimedOut, "receiver signature response timeout")
    })??;
    assert!(receiver_signature_response.contains(r#""id":6"#));
    assert!(
        receiver_signature_response
            .contains(r#""label":"pure rename(label: String, suffix: String = \"!\"): String""#),
        "{receiver_signature_response}"
    );
    assert!(receiver_signature_response.contains(r#""label":"label: String""#));
    assert!(receiver_signature_response.contains(r#""label":"suffix: String = \"!\""#));
    assert!(receiver_signature_response.contains(r#""activeParameter":1"#));
    assert!(receiver_signature_response.contains("Renames a user."));

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": remote_label_position.line,
                    "character": remote_label_position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let imported_receiver_signature_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| {
        std_io::Error::new(
            ErrorKind::TimedOut,
            "imported receiver signature response timeout",
        )
    })??;
    assert!(imported_receiver_signature_response.contains(r#""id":7"#));
    assert!(imported_receiver_signature_response
        .contains(r#""label":"pure remote_label(label: String, suffix: String = \"!\"): String""#));
    assert!(imported_receiver_signature_response.contains(r#""label":"label: String""#));
    assert!(imported_receiver_signature_response.contains(r#""label":"suffix: String = \"!\""#));
    assert!(imported_receiver_signature_response.contains(r#""activeParameter":1"#));
    assert!(imported_receiver_signature_response.contains("Labels a provider record."));

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":8,"method":"shutdown"}"#,
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
