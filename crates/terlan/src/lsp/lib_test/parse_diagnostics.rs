use super::support::*;

/// Verifies the protocol-level go-to-declaration request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document containing one declaration and one same-file call.
///
/// Output:
/// - Test success when `textDocument/declaration` returns the same declaration
///   location as the current definition resolver.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains diagnostics, and
///   requests declaration at the call-site position used by editor clients.
#[tokio::test]
async fn declaration_request_returns_same_document_location() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/declarations.terl","languageId":"terlan","version":1,"text":"module declarations.\n\npub target(): Int ->\n  1.\n\npub caller(): Int ->\n  target().\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/declarations.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/declaration","params":{"textDocument":{"uri":"file:///tmp/declarations.terl"},"position":{"line":6,"character":3}}}"#,
        )
        .await?;
    let declaration_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "declaration response timeout"))??;
    assert!(declaration_response.contains(r#""id":2"#));
    assert!(declaration_response.contains(r#""uri":"file:///tmp/declarations.terl""#));
    assert!(declaration_response.contains(r#""start":{"character":4,"line":2}"#));
    assert!(declaration_response.contains(r#""end":{"character":10,"line":2}"#));

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

/// Verifies the protocol-level go-to-type-definition request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document containing a local type alias and a typed
///   function parameter.
///
/// Output:
/// - Test success when `textDocument/typeDefinition` resolves the parameter
///   annotation to the local type declaration.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains diagnostics, and
///   requests type definition at the type-annotation position used by editor
///   clients.
#[tokio::test]
async fn type_definition_request_returns_same_document_type_location() -> std_io::Result<()> {
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
    let initialize_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "initialize response timeout"))??;
    assert!(
        initialize_response.contains(r#""typeDefinitionProvider":true"#),
        "{initialize_response}"
    );

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )
    .await?;

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/type-definition-protocol.terl","languageId":"terlan","version":1,"text":"module type_definition_protocol.\n\npub type UserId = Int.\n\npub id(value: UserId): UserId ->\n  value.\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(
        &open_message,
        "file:///tmp/type-definition-protocol.terl",
        1,
    );

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/typeDefinition","params":{"textDocument":{"uri":"file:///tmp/type-definition-protocol.terl"},"position":{"line":4,"character":15}}}"#,
        )
        .await?;
    let type_definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "type-definition response timeout"))??;
    assert!(type_definition_response.contains(r#""id":2"#));
    assert!(
        type_definition_response.contains(r#""uri":"file:///tmp/type-definition-protocol.terl""#)
    );
    assert!(type_definition_response.contains(r#""start":{"character":9,"line":2}"#));
    assert!(type_definition_response.contains(r#""end":{"character":15,"line":2}"#));

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

/// Verifies imported types resolve through the protocol type-definition path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A sibling provider `.terli` with a public type declaration.
/// - A consumer document importing and using that type in annotations.
///
/// Output:
/// - Test success when `textDocument/typeDefinition` on the imported type
///   annotation returns the provider type declaration location.
///
/// Transformation:
/// - Exercises imported provider-summary navigation through the actual editor
///   type-definition request path.
#[tokio::test]
async fn type_definition_request_returns_provider_location_for_imported_type_annotation(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-type-definition-protocol-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "module provider.\n\npub type ExternalUser.\n",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_type_definitions.terl"))
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
    let initialize_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "initialize response timeout"))??;
    assert!(
        initialize_response.contains(r#""typeDefinitionProvider":true"#),
        "{initialize_response}"
    );

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )
    .await?;

    let open_payload = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_type_definitions.\n\nimport type provider.{{ExternalUser}}.\n\npub id(value: ExternalUser): ExternalUser ->\n  value.\n"}}}}}}"#,
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

    let type_definition_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/typeDefinition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":4,"character":15}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &type_definition_payload).await?;
    let type_definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "type-definition response timeout"))??;
    assert!(type_definition_response.contains(r#""id":2"#));
    assert!(
        type_definition_response.contains(&format!(
            r#""uri":"{}""#,
            Url::from_file_path(temp_dir.join("provider.terli")).map_err(|()| {
                std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI")
            })?
        )),
        "{type_definition_response}"
    );
    assert!(
        type_definition_response.contains(r#""start":{"character":9,"line":2}"#),
        "{type_definition_response}"
    );
    assert!(
        type_definition_response.contains(r#""end":{"character":21,"line":2}"#),
        "{type_definition_response}"
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

/// Verifies the protocol-level go-to-implementation request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document containing a trait, an explicit impl method, and a
///   receiver call.
///
/// Output:
/// - Test success when `textDocument/implementation` resolves the receiver-call
///   member to the explicit impl method.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains diagnostics, and
///   requests implementation at the receiver-member position used by editor
///   clients.
#[tokio::test]
async fn implementation_request_returns_same_document_impl_method_location() -> std_io::Result<()> {
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
    let initialize_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "initialize response timeout"))??;
    assert!(
        initialize_response.contains(r#""implementationProvider":true"#),
        "{initialize_response}"
    );

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )
    .await?;

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/implementation-protocol.terl","languageId":"terlan","version":1,"text":"module implementation_protocol.\n\npub trait Show[T] {\n  show(value: T): String.\n}.\n\npub struct User {\n  name: String\n}.\n\npub impl Show[User] for User {\n  show(value: User): String ->\n    value.name.\n}.\n\npub caller(value: User): String ->\n  value.show().\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/implementation-protocol.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/implementation","params":{"textDocument":{"uri":"file:///tmp/implementation-protocol.terl"},"position":{"line":16,"character":9}}}"#,
        )
        .await?;
    let implementation_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "implementation response timeout"))??;
    assert!(implementation_response.contains(r#""id":2"#));
    assert!(implementation_response.contains(r#""uri":"file:///tmp/implementation-protocol.terl""#));
    assert!(implementation_response.contains(r#""start":{"character":2,"line":11}"#));
    assert!(implementation_response.contains(r#""end":{"character":6,"line":11}"#));

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

/// Verifies method declarations are excluded from receiver-call references.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document with a trait method, explicit impl method, and
///   receiver call.
///
/// Output:
/// - Test success when `includeDeclaration=false` returns the receiver-call
///   reference without trait or impl declaration locations.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains diagnostics, and
///   requests references at the receiver-member position used by editor
///   clients.
#[tokio::test]
async fn references_request_excludes_trait_and_impl_method_declarations() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/references-method-protocol.terl","languageId":"terlan","version":1,"text":"module references_method_protocol.\n\npub trait Show[T] {\n  show(value: T): String.\n}.\n\npub struct User {\n  name: String\n}.\n\npub impl Show[User] for User {\n  show(value: User): String ->\n    value.name.\n}.\n\npub caller(value: User): String ->\n  value.show().\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(
        &open_message,
        "file:///tmp/references-method-protocol.terl",
        1,
    );

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/references-method-protocol.terl"},"position":{"line":16,"character":9},"context":{"includeDeclaration":false}}}"#,
        )
        .await?;
    let references_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "references response timeout"))??;
    assert!(references_response.contains(r#""id":2"#));
    assert!(references_response.contains(r#""uri":"file:///tmp/references-method-protocol.terl""#));
    assert!(!references_response.contains(r#""start":{"character":2,"line":3}"#));
    assert!(!references_response.contains(r#""start":{"character":2,"line":11}"#));
    assert!(references_response.contains(r#""start":{"character":8,"line":16}"#));
    assert!(references_response.contains(r#""end":{"character":12,"line":16}"#));

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

/// Verifies the protocol-level find-references request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document with one local binding and two later references.
///
/// Output:
/// - Test success when `textDocument/references` returns exact same-document
///   identifier locations and honors `includeDeclaration`.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains diagnostics, and
///   requests references at the local binding declaration used by editor
///   clients.
#[tokio::test]
async fn references_request_returns_same_document_identifier_locations() -> std_io::Result<()> {
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
    let initialize_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "initialize response timeout"))??;
    assert!(
        initialize_response.contains(r#""referencesProvider":true"#),
        "{initialize_response}"
    );

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )
    .await?;

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/references-protocol.terl","languageId":"terlan","version":1,"text":"module references_protocol.\n\npub caller(value: Int): Int ->\n  let next = value + 1;\n      let label = \"next\";\n      let doubled = next + value;\n  doubled + next.\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/references-protocol.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/references-protocol.terl"},"position":{"line":3,"character":6},"context":{"includeDeclaration":true}}}"#,
        )
        .await?;
    let references_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "references response timeout"))??;
    assert!(references_response.contains(r#""id":2"#));
    assert!(references_response.contains(r#""uri":"file:///tmp/references-protocol.terl""#));
    assert!(references_response.contains(r#""start":{"character":6,"line":3}"#));
    assert!(references_response.contains(r#""end":{"character":10,"line":3}"#));
    assert!(!references_response.contains(r#""start":{"character":15,"line":4}"#));
    assert!(references_response.contains(r#""start":{"character":20,"line":5}"#));
    assert!(references_response.contains(r#""end":{"character":24,"line":5}"#));
    assert!(references_response.contains(r#""start":{"character":12,"line":6}"#));
    assert!(references_response.contains(r#""end":{"character":16,"line":6}"#));

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{"textDocument":{"uri":"file:///tmp/references-protocol.terl"},"position":{"line":3,"character":6},"context":{"includeDeclaration":false}}}"#,
        )
        .await?;
    let references_without_declaration_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| {
        std_io::Error::new(
            ErrorKind::TimedOut,
            "references without declaration response timeout",
        )
    })??;
    assert!(references_without_declaration_response.contains(r#""id":3"#));
    assert!(
        !references_without_declaration_response.contains(r#""start":{"character":6,"line":3}"#)
    );
    assert!(
        references_without_declaration_response.contains(r#""start":{"character":20,"line":5}"#)
    );
    assert!(
        references_without_declaration_response.contains(r#""start":{"character":12,"line":6}"#)
    );

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
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

/// Verifies imported use-sites are preserved when declarations are excluded.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document with a selected import and one imported call.
///
/// Output:
/// - Test success when `includeDeclaration=false` drops the import item but
///   keeps the cursor use-site reference.
///
/// Transformation:
/// - Starts the real LSP service with a sibling provider summary, opens the
///   consumer document, and requests references from the imported call site.
#[tokio::test]
async fn references_request_preserves_imported_use_site_without_declaration() -> std_io::Result<()>
{
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-references-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "module provider.\n\npub to_string(value: Int): String.\n",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_references.terl"))
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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_references.\n\nimport provider.{{to_string}}.\n\npub caller(): String ->\n  to_string(1).\n"}}}}}}"#,
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

    let references_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":4}},"context":{{"includeDeclaration":false}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &references_payload).await?;
    let references_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "references response timeout"))??;
    assert!(references_response.contains(r#""id":2"#));
    assert!(references_response.contains(&format!(r#""uri":"{}""#, uri)));
    assert!(
        !references_response.contains(r#""start":{"character":17,"line":2}"#),
        "{references_response}"
    );
    assert!(
        references_response.contains(r#""start":{"character":2,"line":5}"#),
        "{references_response}"
    );
    assert!(
        references_response.contains(r#""end":{"character":11,"line":5}"#),
        "{references_response}"
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
