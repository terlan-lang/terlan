
/// Verifies completion tolerates incomplete syntax.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A syntactically incomplete Terlan buffer with a cursor at the end.
///
/// Output:
/// - Test success when `textDocument/completion` returns an empty result
///   instead of an LSP error or stale cached completion items.
///
/// Transformation:
/// - Exercises didOpen diagnostics plus a completion request against the same
///   broken buffer so editors can safely ask for completions while users type.
#[tokio::test]
async fn completion_request_handles_incomplete_syntax_without_stale_items() -> std_io::Result<()> {
    let uri = "file:///tmp/incomplete-completion.terl";
    let source = "\
module incomplete_completion.

pub caller(): Int ->
    let value = 1;
    ";
    let position = Position::new(4, 4);

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
                    "uri": uri,
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
    assert_parse_diagnostic_message(&open_message, uri, 1);

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": position.line,
                    "character": position.character
                }
            }
        })
        .to_string(),
    )
    .await?;
    let completion_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "completion response timeout"))??;
    assert!(completion_response.contains(r#""id":2"#));
    assert!(
        completion_response.contains(r#""result":[]"#),
        "{completion_response}"
    );
    assert!(
        !completion_response.contains(r#""error""#),
        "{completion_response}"
    );

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

    Ok(())
}

/// Verifies ambiguous completion labels prefer local declarations.
///
/// Inputs:
/// - A source module defining `lookup/0` while importing provider `lookup/1`.
/// - A temporary provider `.terli` exposing the imported function metadata.
///
/// Output:
/// - Test success when the completion list contains both ambiguous labels and
///   ranks the local function before the imported function.
///
/// Transformation:
/// - Exercises the completion ordering path used for editor ranking without
///   relying on a client-side sort order.
#[test]
fn completion_ranks_local_symbol_before_imported_ambiguous_symbol() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-ambiguous-completion-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

/// Imported lookup.
pub lookup(id: Int): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module consumer.

import provider.{lookup}.

/// Local lookup.
pub lookup(): Int ->
  1.

pub caller(): Int ->
  lookup().
";
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: text.to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    let offset = text
        .find("  lookup().")
        .expect("ambiguous completion marker")
        + "  ".len();
    let prefix = &text[..offset];
    let position = Position::new(
        prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
        prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32,
    );

    let items = Backend::completion_items_for_position(&uri, &document, position);
    let lookup_items = items
        .iter()
        .filter(|item| item.label == "lookup")
        .collect::<Vec<_>>();

    assert!(
        lookup_items.len() >= 2,
        "expected local and imported lookup completions: {lookup_items:?}"
    );
    assert_eq!(
        lookup_items[0].detail.as_deref(),
        Some("function lookup/0 -> Int")
    );
    assert_eq!(
        lookup_items[1].detail.as_deref(),
        Some("function provider.lookup/1 -> String")
    );
    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

/// Verifies overloaded receiver methods remain visible in completion.
///
/// Inputs:
/// - A source module with two public receiver methods named `label` on the
///   same receiver type but with different arities.
///
/// Output:
/// - Test success when receiver completion returns both overloads with distinct
///   method details.
///
/// Transformation:
/// - Exercises receiver-method completion without client-side deduplication so
///   overloaded method surfaces remain discoverable in editors.
#[test]
fn completion_preserves_overloaded_receiver_method_items() -> std_io::Result<()> {
    let uri = Url::parse("file:///tmp/overloaded-method-completion.terl")
        .map_err(|err| std_io::Error::new(ErrorKind::InvalidInput, err))?;
    let text = "\
module overloaded_methods.

pub struct User {
  name: String
}.

pub (user: User) label(): String ->
  user.name.

pub (user: User) label(suffix: String): String ->
  user.name + suffix.

pub caller(user: User): String ->
  user.label().
";
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: text.to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    let offset = text
        .find("  user.label().")
        .expect("overloaded receiver completion marker")
        + "  user.".len();
    let prefix = &text[..offset];
    let position = Position::new(
        prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
        prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32,
    );

    let items = Backend::completion_items_for_position(&uri, &document, position);
    let label_items = items
        .iter()
        .filter(|item| item.label == "label")
        .collect::<Vec<_>>();

    assert_eq!(
        label_items.len(),
        2,
        "expected both receiver overloads: {label_items:?}"
    );
    assert_eq!(
        label_items[0].detail.as_deref(),
        Some("method User.label/0 -> String")
    );
    assert_eq!(
        label_items[1].detail.as_deref(),
        Some("method User.label/1 -> String")
    );

    Ok(())
}

/// Verifies completion positions survive formatter layout changes.
///
/// Inputs:
/// - Compact Terlan source that the formatter expands into a multiline
///   function body.
///
/// Output:
/// - Test success when completion computed from the formatted cursor position
///   still includes the local let binding.
///
/// Transformation:
/// - Runs the source through the compiler formatter first, then uses the
///   formatted text as the editor buffer for completion.
#[test]
fn completion_uses_formatter_shifted_positions_for_local_functions() -> std_io::Result<()> {
    let formatted = format_source_module(
        "\
module formatter_shift.

pub shifted(): Unit ->
    Unit.

pub caller(): Unit -> shifted(); shifted().
",
    )
    .map_err(|err| std_io::Error::new(ErrorKind::InvalidData, err.message))?;
    let marker = "shifted();\n    ";
    let offset = formatted
        .find(marker)
        .unwrap_or_else(|| panic!("formatted source missing marker {marker:?}:\n{formatted}"))
        + marker.len();
    let prefix = &formatted[..offset];
    let position = Position::new(
        prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
        prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32,
    );
    let uri = Url::parse("file:///tmp/formatter-shift-completion.terl")
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

    let items = Backend::completion_items_for_position(&uri, &document, position);
    let shifted = items
        .iter()
        .find(|item| item.label == "shifted")
        .expect("formatted local function completion");

    assert_eq!(shifted.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(
        shifted.detail.as_deref(),
        Some("function shifted/0 -> Unit")
    );

    Ok(())
}

/// Verifies generated interface summaries feed completion provenance.
///
/// Inputs:
/// - A generated summary file `pkg.math.typi` declaring module `pkg.math`.
/// - A source module importing `pkg.math.{add}`.
///
/// Output:
/// - Test success when completion exposes the imported function with
///   package-qualified detail.
///
/// Transformation:
/// - Exercises generated-summary discovery for completion without relying on
///   hand-written sibling `.terli` files.
#[test]
fn completion_uses_generated_typi_interface_summary_provenance() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-package-completion-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("pkg.math.typi"),
        "\
module pkg.math.

pub add(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module consumer.

import pkg.math.{add}.

pub caller(): Int ->
  add(1, 2).
";
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: text.to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    let offset = text
        .find("  add(1, 2).")
        .expect("package completion marker")
        + "  ".len();
    let prefix = &text[..offset];
    let position = Position::new(
        prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
        prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32,
    );

    let items = Backend::completion_items_for_position(&uri, &document, position);
    let add = items
        .iter()
        .find(|item| item.label == "add")
        .expect("package summary function completion");

    assert_eq!(add.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(
        add.detail.as_deref(),
        Some("function pkg.math.add/2 -> Int")
    );
    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

/// Verifies deleted generated summaries do not leave stale completion entries.
///
/// Inputs:
/// - A generated summary file `pkg.stale.typi` declaring `gone/0`.
/// - A source module importing `pkg.stale.{gone}` and declaring a local binding.
///
/// Output:
/// - Test success when the first completion request sees `gone`, the second
///   request after deleting the summary does not see `gone`, and local
///   completions remain available.
///
/// Transformation:
/// - Exercises generated-summary completion discovery twice against the same
///   document URI so editor completions cannot retain deleted package metadata.
#[test]
fn completion_rejects_deleted_generated_typi_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-deleted-package-completion-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let summary_path = temp_dir.join("pkg.stale.typi");
    fs::write(
        &summary_path,
        "\
module pkg.stale.

pub gone(): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module consumer.

import pkg.stale.{gone}.

pub caller(): Int ->
  let local_value = 1;
  local_value.
";
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: text.to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    let offset = text
        .find("  local_value.")
        .expect("deleted package completion marker")
        + "  ".len();
    let prefix = &text[..offset];
    let position = Position::new(
        prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
        prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32,
    );

    let fresh_items = Backend::completion_items_for_position(&uri, &document, position);
    assert!(
        fresh_items.iter().any(|item| item.label == "gone"),
        "generated package completion should be present before deletion: {fresh_items:?}"
    );

    fs::remove_file(&summary_path)?;
    let stale_items = Backend::completion_items_for_position(&uri, &document, position);
    assert!(
        stale_items.iter().all(|item| item.label != "gone"),
        "deleted generated package completion must not remain stale: {stale_items:?}"
    );
    assert!(
        stale_items.iter().any(|item| item.label == "local_value"),
        "local completion should remain available after package summary deletion: {stale_items:?}"
    );
    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

/// Verifies mixed target-profile imports do not leak incompatible completions.
///
/// Inputs:
/// - Generated summaries for `std.js.Promise` and `std.vm.Task`.
/// - A source module importing both target families while editing a local
///   function body.
///
/// Output:
/// - Test success when local completions remain available but target-specific
///   imported completions are suppressed.
///
/// Transformation:
/// - Exercises compiler-owned target inference from syntax imports before LSP
///   completion projects imported interface summaries.
#[test]
fn completion_rejects_mixed_target_profile_imported_suggestions() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-target-profile-completion-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("std.js.Promise.typi"),
        "\
module std.js.Promise.

pub then(value: Dynamic): Dynamic.
",
    )?;
    fs::write(
        temp_dir.join("std.vm.Task.typi"),
        "\
module std.vm.Task.

pub spawn(value: Dynamic): Dynamic.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("mixed.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module mixed.

import std.js.Promise.{then}.
import std.vm.Task.{spawn}.

pub local(): Int ->
  let marker = 1;
  marker.
";
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: text.to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    let offset = text
        .find("  marker.")
        .expect("target profile completion marker")
        + "  ".len();
    let prefix = &text[..offset];
    let position = Position::new(
        prefix
            .as_bytes()
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
        prefix.rsplit('\n').next().unwrap_or(prefix).chars().count() as u32,
    );

    let items = Backend::completion_items_for_position(&uri, &document, position);
    assert!(
        items.iter().any(|item| item.label == "marker"),
        "local completion should remain available during target conflict: {items:?}"
    );
    for leaked in ["then", "spawn"] {
        assert!(
            items.iter().all(|item| item.label != leaked),
            "target-incompatible completion `{leaked}` should be suppressed: {items:?}"
        );
    }
    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}
