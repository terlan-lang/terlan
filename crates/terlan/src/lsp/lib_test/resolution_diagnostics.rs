use super::support::*;

/// Verifies imported struct fields resolve through protocol-level declaration.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A sibling provider `.terli` with a public struct and public field.
/// - A consumer document importing the struct type and reading the field through
///   a typed receiver.
///
/// Output:
/// - Test success when `textDocument/declaration` on `user.name` returns the
///   provider field declaration location.
///
/// Transformation:
/// - Locks declaration-provider behavior to the same imported struct-field
///   resolver used by go-to-definition.
#[tokio::test]
async fn declaration_request_returns_provider_location_for_imported_struct_field_reference(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-field-declaration-protocol-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

pub struct ExternalUser {
    name: String,
    #secret: String
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_field_declarations.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;

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

    let open_payload = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_field_declarations.\n\nimport type provider.{{ExternalUser}}.\n\npub user_name(user: ExternalUser): String ->\n  user.name.\n"}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &open_payload).await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, uri.as_str(), 1);

    let declaration_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/declaration","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":8}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &declaration_payload).await?;
    let declaration_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "declaration response timeout"))??;
    assert!(declaration_response.contains(r#""id":2"#));
    assert!(
        declaration_response.contains(&format!(
            r#""uri":"{}""#,
            Url::from_file_path(temp_dir.join("provider.terli")).map_err(|()| {
                std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI")
            })?
        )),
        "{declaration_response}"
    );
    assert!(
        declaration_response.contains(r#""start":{"character":4,"line":3}"#),
        "{declaration_response}"
    );
    assert!(
        declaration_response.contains(r#""end":{"character":8,"line":3}"#),
        "{declaration_response}"
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;
    fs::remove_dir_all(&temp_dir)?;

    Ok(())
}

/// Verifies template documents do not expose Terlan definition targets.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A template document opened with a template language id.
///
/// Output:
/// - Test success when `textDocument/definition` returns an empty location
///   list.
///
/// Transformation:
/// - Exercises the protocol path used by editors so template buffers stay
///   diagnostic-capable without entering source-module definition lookup.
#[tokio::test]
async fn definition_request_returns_empty_for_template_documents() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/page.terl.html","languageId":"terlan-template-html","version":1,"text":"<main>${title}</main>\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/page.terl.html", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":"file:///tmp/page.terl.html"},"position":{"line":0,"character":9}}}"#,
        )
        .await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(definition_response.contains(r#""result":[]"#));

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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

/// Verifies template documents do not expose Terlan reference targets.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A template document opened with a template language id.
///
/// Output:
/// - Test success when `textDocument/references` returns an empty location
///   list.
///
/// Transformation:
/// - Exercises the protocol path used by editors so template buffers do not
///   reuse source-module reference lookup by accident.
#[tokio::test]
async fn references_request_returns_empty_for_template_documents() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/page-references.terl.html","languageId":"terlan-template-html","version":1,"text":"<main>${title}</main>\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/page-references.terl.html", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/page-references.terl.html"},"position":{"line":0,"character":9},"context":{"includeDeclaration":true}}}"#,
        )
        .await?;
    let references_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "references response timeout"))??;
    assert!(references_response.contains(r#""id":2"#));
    assert!(references_response.contains(r#""result":[]"#));

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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

#[tokio::test]
async fn did_open_reports_parse_diagnostic() -> std_io::Result<()> {
    let (mut client_to_server, server_stdin) = duplex(2048);
    let (server_stdout, mut client_stdout) = duplex(2048);

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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/diag.terl","languageId":"terlan","version":1,"text":"module broken"}}}"#,
        )
        .await?;

    let publish_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "diagnostics message timeout"))??;
    assert_parse_diagnostic_message(&publish_message, "file:///tmp/diag.terl", 1);

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

#[tokio::test]
async fn did_open_reports_diagnostic_and_clear_on_parse_fix() -> std_io::Result<()> {
    let (mut client_to_server, server_stdin) = duplex(2048);
    let (server_stdout, mut client_stdout) = duplex(2048);

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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/diag.terl","languageId":"terlan","version":1,"text":"module broken"}}}"#,
        )
        .await?;

    let parse_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "diagnostics message timeout"))??;
    assert_parse_diagnostic_message(&parse_message, "file:///tmp/diag.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///tmp/diag.terl","version":2},"contentChanges":[{"text":"module fixed.\n"}]}}"#,
        )
        .await?;

    let clear_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "diagnostics clear timeout"))??;
    assert_clear_diagnostic_message(&clear_message, "file:///tmp/diag.terl", 2);

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

#[tokio::test]
async fn did_open_reports_type_diagnostic() -> std_io::Result<()> {
    let (mut client_to_server, server_stdin) = duplex(2048);
    let (server_stdout, mut client_stdout) = duplex(2048);

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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/type-diag.terl","languageId":"terlan","version":1,"text":"module type_diag.\n\npub bad(X: Int): Binary ->\n    X + 1.\n"}}}"#,
        )
        .await?;

    let publish_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "type diagnostics timeout"))??;
    assert_type_diagnostic_message(&publish_message, "file:///tmp/type-diag.terl", 1);

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

/// Verifies resolver diagnostics are published to LSP clients.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A syntactically valid Terlan document with duplicate type declarations.
///
/// Output:
/// - Test success when the client receives a `terlan-hir` publishDiagnostics
///   notification for the opened document.
///
/// Transformation:
/// - Starts the real language server, opens a document that fails HIR
///   resolution, and checks the serialized diagnostics notification.
#[tokio::test]
async fn did_open_reports_resolve_diagnostic() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/resolve-diag.terl","languageId":"terlan","version":1,"text":"module resolve_diag.\n\ntype A = ok.\ntype A = error.\n"}}}"#,
        )
        .await?;

    let publish_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "resolve diagnostics timeout"))??;
    assert_resolve_diagnostic_message(&publish_message, "file:///tmp/resolve-diag.terl", 1);

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

#[tokio::test]
async fn did_open_reports_type_warning() -> std_io::Result<()> {
    let (mut client_to_server, server_stdin) = duplex(2048);
    let (server_stdout, mut client_stdout) = duplex(2048);

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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/type-warning.terl","languageId":"terlan","version":1,"text":"module type_warning.\n\npub type OptionInt =\n      none\n    | {some, Int}.\n\npub unwrap(M: OptionInt): Int.\n\nunwrap({some, X}) ->\n    X.\n"}}}"#,
        )
        .await?;

    let publish_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "type warning timeout"))??;
    assert_type_warning_message(&publish_message, "file:///tmp/type-warning.terl", 1);

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}

#[tokio::test]
async fn did_change_reports_parse_diagnostic() -> std_io::Result<()> {
    let (mut client_to_server, server_stdin) = duplex(2048);
    let (server_stdout, mut client_stdout) = duplex(2048);

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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/change-diag.terl","languageId":"terlan","version":1,"text":"module change_diag.\n"}}}"#,
        )
        .await?;

    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/change-diag.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///tmp/change-diag.terl","version":2},"contentChanges":[{"text":"module broken"}]}}"#,
        )
        .await?;

    let change_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "change diagnostics timeout"))??;
    assert_parse_diagnostic_message(&change_message, "file:///tmp/change-diag.terl", 2);

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
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
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    )
    .await?;

    server_task.abort();
    let _ = server_task.await;

    Ok(())
}
