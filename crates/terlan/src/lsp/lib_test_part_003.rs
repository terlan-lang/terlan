
/// Verifies imported references resolve to provider interface locations.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid source document that calls an imported standard module member.
///
/// Output:
/// - Test success when `textDocument/definition` returns the provider
///   declaration location for the imported reference.
///
/// Transformation:
/// - Locks the editor navigation path for selected public imports backed by
///   sibling `.terli` provider summaries.
#[tokio::test]
async fn definition_request_returns_provider_location_for_imported_reference() -> std_io::Result<()>
{
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "module provider.\n\npub to_string(value: Int): String.\n",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_definitions.terl"))
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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_definitions.\n\nimport provider.{{to_string}}.\n\npub caller(): String ->\n  to_string(1).\n"}}}}}}"#,
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

    let definition_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":4}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &definition_payload).await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(
        definition_response.contains(&format!(
            r#""uri":"{}""#,
            Url::from_file_path(temp_dir.join("provider.terli")).map_err(|()| {
                std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI")
            })?
        )),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""start":{"character":4,"line":2}"#),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""end":{"character":13,"line":2}"#),
        "{definition_response}"
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

/// Verifies imported struct fields resolve through the protocol definition path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A sibling provider `.terli` with a public struct and public field.
/// - A consumer document importing the struct type and reading the field through
///   a typed receiver.
///
/// Output:
/// - Test success when `textDocument/definition` on `user.name` returns the
///   provider field declaration location.
///
/// Transformation:
/// - Exercises the actual editor protocol path for imported struct-field
///   navigation instead of only the helper-level resolver path.
#[tokio::test]
async fn definition_request_returns_provider_location_for_imported_struct_field_reference(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-field-definition-protocol-{}-{unique}",
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
    let uri = Url::from_file_path(temp_dir.join("imported_field_definitions.terl"))
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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_field_definitions.\n\nimport type provider.{{ExternalUser}}.\n\npub user_name(user: ExternalUser): String ->\n  user.name.\n"}}}}}}"#,
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

    let definition_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":8}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &definition_payload).await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(
        definition_response.contains(&format!(
            r#""uri":"{}""#,
            Url::from_file_path(temp_dir.join("provider.terli")).map_err(|()| {
                std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI")
            })?
        )),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""start":{"character":4,"line":3}"#),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""end":{"character":8,"line":3}"#),
        "{definition_response}"
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

/// Verifies imported shape references resolve through the protocol definition path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A sibling provider `.terli` with a public shape declaration.
/// - A source document importing and referencing that shape.
///
/// Output:
/// - Test success when `textDocument/definition` returns the provider shape
///   declaration location.
///
/// Transformation:
/// - Locks protocol-level editor navigation to the same provider-summary shape
///   resolver already used by helper-level definition lookup.
#[tokio::test]
async fn definition_request_returns_provider_location_for_imported_shape_reference(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-shape-definition-protocol-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "module provider.\n\npub shape UserAsset(id) = \"users/${id}/asset\".\n",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_shapes.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;

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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_shapes.\n\nimport provider.{{UserAsset}}.\n\npub route_name(): Dynamic ->\n  UserAsset.\n"}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &open_payload).await?;
    let _ = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;

    let definition_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":3}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &definition_payload).await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(
        definition_response.contains(&format!(r#""uri":"{}""#, provider_uri)),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""start":{"character":10,"line":2}"#),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""end":{"character":19,"line":2}"#),
        "{definition_response}"
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

/// Verifies imported constructor calls resolve through the protocol definition path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A sibling provider `.terli` with a public constructor declaration.
/// - A source document importing and calling that constructor.
///
/// Output:
/// - Test success when `textDocument/definition` returns the provider
///   constructor declaration location.
///
/// Transformation:
/// - Locks protocol-level constructor navigation to provider-summary visibility
///   so editor clients do not need a helper-only path for constructor imports.
#[tokio::test]
async fn definition_request_returns_provider_location_for_imported_constructor_reference(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-constructor-definition-protocol-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub type ExternalUser.

pub constructor BuildExternalUser {
    (name: String): ExternalUser ->
        terlan_interface_constructor
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_constructors.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;

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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_constructors.\n\nimport provider.{{BuildExternalUser}}.\n\npub made(): Dynamic ->\n  BuildExternalUser(\"Ada\").\n"}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &open_payload).await?;
    let _ = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;

    let definition_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":3}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &definition_payload).await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(
        definition_response.contains(&format!(r#""uri":"{}""#, provider_uri)),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""start":{"character":16,"line":4}"#),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""end":{"character":33,"line":4}"#),
        "{definition_response}"
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

/// Verifies imported trait references resolve through the protocol definition path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A sibling provider `.terli` with a public trait declaration.
/// - A source document importing and referencing that trait.
///
/// Output:
/// - Test success when `textDocument/definition` returns the provider trait
///   declaration location.
///
/// Transformation:
/// - Locks protocol-level trait navigation to provider-summary visibility so
///   helper-level and editor-client definition behavior stay equivalent.
#[tokio::test]
async fn definition_request_returns_provider_location_for_imported_trait_reference(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-trait-definition-protocol-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub trait Named[T] {
    name(value: T): String.
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_traits.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;

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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_traits.\n\nimport provider.{{Named}}.\n\npub trait_name(): Dynamic ->\n  Named.\n"}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &open_payload).await?;
    let _ = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;

    let definition_payload = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":3}}}}}}"#,
        uri
    );
    write_lsp_message(&mut client_to_server, &definition_payload).await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(
        definition_response.contains(&format!(r#""uri":"{}""#, provider_uri)),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""start":{"character":10,"line":2}"#),
        "{definition_response}"
    );
    assert!(
        definition_response.contains(r#""end":{"character":15,"line":2}"#),
        "{definition_response}"
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

/// Verifies imported references resolve through the protocol-level declaration
/// request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid source document that calls a selected imported provider function.
///
/// Output:
/// - Test success when `textDocument/declaration` returns the provider
///   declaration location for the imported reference.
///
/// Transformation:
/// - Locks declaration-provider behavior to the same public provider-summary
///   resolver used by go-to-definition, matching Terlan's current declaration
///   and definition model.
#[tokio::test]
async fn declaration_request_returns_provider_location_for_imported_reference() -> std_io::Result<()>
{
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-declaration-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "module provider.\n\npub to_string(value: Int): String.\n",
    )?;
    let uri = Url::from_file_path(temp_dir.join("imported_declarations.terl"))
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
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"terlan","version":1,"text":"module imported_declarations.\n\nimport provider.{{to_string}}.\n\npub caller(): String ->\n  to_string(1).\n"}}}}}}"#,
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
        r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/declaration","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":5,"character":4}}}}}}"#,
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
        declaration_response.contains(r#""start":{"character":4,"line":2}"#),
        "{declaration_response}"
    );
    assert!(
        declaration_response.contains(r#""end":{"character":13,"line":2}"#),
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
