use super::document::{DocumentKind, OpenDocument, OpenDocuments};
use super::Backend;
use crate::terlan_syntax::Span;
use std::fs;
use std::io::{self as std_io, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::time::{timeout, Duration};
use tower_lsp::lsp_types::{Position, SymbolKind, Url};
use tower_lsp::{LspService, Server};

/// Writes one framed JSON-RPC message to the in-memory LSP stream.
///
/// Inputs:
/// - `writer`: duplex stream connected to the test server input.
/// - `payload`: raw JSON-RPC request or notification body.
///
/// Output:
/// - `Ok(())` when the framed message is flushed.
///
/// Transformation:
/// - Prefixes the payload with an LSP `Content-Length` header and writes the
///   complete frame to the stream.
async fn write_lsp_message(writer: &mut DuplexStream, payload: &str) -> std_io::Result<()> {
    let mut out = Vec::with_capacity(payload.len() + 64);
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
    out.extend_from_slice(payload.as_bytes());
    writer.write_all(&out).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads one framed JSON-RPC message from the in-memory LSP stream.
///
/// Inputs:
/// - `reader`: duplex stream connected to the test server output.
///
/// Output:
/// - UTF-8 JSON-RPC body.
///
/// Transformation:
/// - Reads LSP headers until the blank line, extracts `Content-Length`, then
///   reads exactly that many body bytes.
async fn read_lsp_message(reader: &mut DuplexStream) -> std_io::Result<String> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        reader.read_exact(&mut byte).await?;
        header.push(byte[0]);
        if header.len() >= 4 && header[header.len() - 4..] == *b"\r\n\r\n" {
            break;
        }
    }

    let header_str = String::from_utf8_lossy(&header);
    let content_length = header_str
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        })
        .ok_or_else(|| std_io::Error::new(ErrorKind::InvalidData, "missing content-length"))?;

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await?;
    Ok(String::from_utf8(body).map_err(|err| std_io::Error::new(ErrorKind::InvalidData, err))?)
}

/// Asserts that a serialized notification contains a syntax parse diagnostic.
///
/// Inputs:
/// - `message`: raw JSON-RPC notification emitted by the test server.
/// - `uri`: expected document URI.
/// - `version`: expected document version.
///
/// Output:
/// - Panics when the notification does not match the syntax diagnostic shape.
///
/// Transformation:
/// - Checks stable protocol substrings without depending on full JSON field
///   ordering.
fn assert_parse_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":1"#));
    assert!(message.contains(r#""source":"terlan-syntax""#));
}

/// Asserts that a serialized notification contains a typechecker diagnostic.
///
/// Inputs:
/// - `message`: raw JSON-RPC notification emitted by the test server.
/// - `uri`: expected document URI.
/// - `version`: expected document version.
///
/// Output:
/// - Panics when the notification does not match the type error shape.
///
/// Transformation:
/// - Checks stable protocol substrings for editor-facing type diagnostics.
fn assert_type_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":1"#));
    assert!(message.contains(r#""source":"terlan-typeck""#));
}

/// Asserts that a serialized LSP notification contains a resolver diagnostic.
///
/// Inputs:
/// - `message`: raw JSON-RPC message emitted by the test LSP server.
/// - `uri`: expected document URI.
/// - `version`: expected document version.
///
/// Output:
/// - Panics when the notification does not contain the expected resolver
///   diagnostic markers.
///
/// Transformation:
/// - Performs a protocol-level smoke check on the serialized diagnostic
///   payload without depending on JSON field ordering beyond stable substrings.
fn assert_resolve_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":1"#));
    assert!(message.contains(r#""source":"terlan-hir""#));
}

/// Asserts that a serialized notification contains a typechecker warning.
///
/// Inputs:
/// - `message`: raw JSON-RPC notification emitted by the test server.
/// - `uri`: expected document URI.
/// - `version`: expected document version.
///
/// Output:
/// - Panics when the notification does not match the type warning shape.
///
/// Transformation:
/// - Checks stable protocol substrings for warning severity and typechecker
///   diagnostic source.
fn assert_type_warning_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":2"#));
    assert!(message.contains(r#""source":"terlan-typeck""#));
}

/// Asserts that a serialized LSP notification contains a template diagnostic.
///
/// Inputs:
/// - `message`: raw JSON-RPC message emitted by the test LSP server.
/// - `uri`: expected document URI.
/// - `version`: expected document version.
///
/// Output:
/// - Panics when the notification does not contain the expected template
///   diagnostic markers.
///
/// Transformation:
/// - Performs a protocol-level smoke check on the serialized diagnostic
///   payload without depending on JSON field ordering beyond stable substrings.
fn assert_template_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":1"#));
    assert!(message.contains(r#""source":"terlan-template""#));
}

/// Asserts that a serialized notification clears diagnostics for a document.
///
/// Inputs:
/// - `message`: raw JSON-RPC notification emitted by the test server.
/// - `uri`: expected document URI.
/// - `version`: expected document version.
///
/// Output:
/// - Panics when the notification does not contain an empty diagnostics list.
///
/// Transformation:
/// - Checks the LSP clear-diagnostics notification shape used after clean
///   opens and document close events.
fn assert_clear_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(
        message.contains(r#""diagnostics":[]"#),
        "expected clear diagnostics message, got: {message}"
    );
}

#[tokio::test]
async fn smoke_initialize_and_shutdown() -> std_io::Result<()> {
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
    let initialize_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "initialize response timeout"))??;
    assert!(initialize_response.contains(r#""id":1"#));
    assert!(initialize_response.contains(r#""result""#));
    assert!(initialize_response.contains(r#""textDocumentSync":1"#));
    assert!(initialize_response.contains(r#""documentSymbolProvider":true"#));
    assert!(initialize_response.contains(r#""definitionProvider":true"#));
    assert!(initialize_response.contains(r#""hoverProvider":true"#));
    assert!(initialize_response.contains(r#""codeActionProvider":true"#));

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    )
    .await?;

    write_lsp_message(
        &mut client_to_server,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#,
    )
    .await?;

    let shutdown_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "shutdown response timeout"))??;
    assert!(shutdown_response.contains(r#""id":2"#));
    assert!(shutdown_response.contains(r#""result":null"#));

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
async fn did_open_is_accepted() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/opened.terl","languageId":"terlan","version":1,"text":"module opened.\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/opened.terl", 1);

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

/// Verifies template documents publish clear diagnostics instead of parse errors.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - One `.terl.html` document opened with a template language id.
///
/// Output:
/// - Test success when the server publishes an empty diagnostics notification.
///
/// Transformation:
/// - Exercises the same LSP document-open path editor packages use for
///   templates while keeping target-aware template diagnostics deferred.
#[tokio::test]
async fn did_open_template_document_publishes_clear_diagnostics() -> std_io::Result<()> {
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

/// Verifies invalid template structure publishes template diagnostics.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - One malformed `.terl.json` document opened with a template language id.
///
/// Output:
/// - Test success when the server publishes a `terlan-template` diagnostic.
///
/// Transformation:
/// - Exercises LSP reuse of the shared `terlan_html` artifact-template
///   validators without parsing template bodies as Terlan source modules.
#[tokio::test]
async fn did_open_invalid_template_document_publishes_template_diagnostic() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/page.terl.json","languageId":"terlan-template-json","version":1,"text":"{\"title\": [\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_template_diagnostic_message(&open_message, "file:///tmp/page.terl.json", 1);
    assert!(open_message.contains("invalid JSON template structure"));

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

/// Verifies the protocol-level document-symbol request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document containing a module, type alias, and function.
///
/// Output:
/// - Test success when `textDocument/documentSymbol` returns the expected
///   nested symbol response through JSON-RPC.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains the diagnostics
///   publish, requests document symbols, and checks the serialized response
///   names/ranges without bypassing the language-server protocol.
#[tokio::test]
async fn document_symbol_request_returns_nested_symbols() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/symbols.terl","languageId":"terlan","version":1,"text":"module symbols.Main.\n\npub type UserId = Int.\n\npub count(): Int ->\n  1.\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/symbols.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///tmp/symbols.terl"}}}"#,
        )
        .await?;
    let symbols_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "document symbol response timeout"))??;
    assert!(symbols_response.contains(r#""id":2"#));
    assert!(symbols_response.contains(r#""result":[{"#));
    assert!(symbols_response.contains(r#""name":"symbols.Main""#));
    assert!(symbols_response.contains(r#""children":[{"#));
    assert!(symbols_response.contains(r#""name":"UserId""#));
    assert!(symbols_response.contains(r#""name":"count""#));
    assert!(symbols_response.contains(r#""selectionRange""#));

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

/// Verifies template documents do not expose Terlan source symbols.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A template document opened with a template language id.
///
/// Output:
/// - Test success when `textDocument/documentSymbol` returns an empty list.
///
/// Transformation:
/// - Exercises the protocol path used by editors so template buffers can share
///   the LSP server without being treated as source modules for navigation.
#[tokio::test]
async fn document_symbol_request_returns_empty_for_template_documents() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"file:///tmp/page.terl.html"}}}"#,
        )
        .await?;
    let symbols_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "document symbol response timeout"))??;
    assert!(symbols_response.contains(r#""id":2"#));
    assert!(symbols_response.contains(r#""result":[]"#));

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

/// Verifies the protocol-level go-to-definition request path.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid Terlan document containing one declaration and one same-file call.
///
/// Output:
/// - Test success when `textDocument/definition` returns the declaration
///   location through JSON-RPC.
///
/// Transformation:
/// - Starts the real LSP service, opens a document, drains diagnostics, and
///   requests definition at the call-site position used by editor clients.
#[tokio::test]
async fn definition_request_returns_same_document_location() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/definitions.terl","languageId":"terlan","version":1,"text":"module definitions.\n\npub target(): Int ->\n  1.\n\npub caller(): Int ->\n  target().\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/definitions.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":"file:///tmp/definitions.terl"},"position":{"line":6,"character":3}}}"#,
        )
        .await?;
    let definition_response = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "definition response timeout"))??;
    assert!(definition_response.contains(r#""id":2"#));
    assert!(definition_response.contains(r#""uri":"file:///tmp/definitions.terl""#));
    assert!(definition_response.contains(r#""start":{"character":4,"line":2}"#));
    assert!(definition_response.contains(r#""end":{"character":10,"line":2}"#));

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

/// Verifies imported references do not pretend to have cross-file locations.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A valid source document that calls an imported standard module member.
///
/// Output:
/// - Test success when `textDocument/definition` returns an empty location
///   list for the imported reference.
///
/// Transformation:
/// - Locks the current 0.0.5 navigation boundary: same-document declarations
///   may resolve, but cross-file imports stay empty until the compiler exposes
///   stable source locations for provider interfaces.
#[tokio::test]
async fn definition_request_returns_empty_for_imported_reference() -> std_io::Result<()> {
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

#[tokio::test]
async fn did_change_is_accepted() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/changed.terl","languageId":"terlan","version":1,"text":"module changed.\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/changed.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///tmp/changed.terl","version":2},"contentChanges":[{"text":"module changed.\n"}]}}"#,
        )
        .await?;
    let change_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "change diagnostics timeout"))??;
    assert_clear_diagnostic_message(&change_message, "file:///tmp/changed.terl", 2);

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
async fn did_close_is_accepted() -> std_io::Result<()> {
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/closed.terl","languageId":"terlan","version":1,"text":"module closed"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_parse_diagnostic_message(&open_message, "file:///tmp/closed.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":"file:///tmp/closed.terl"}}}"#,
        )
        .await?;
    let close_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "close diagnostics timeout"))??;
    assert_clear_diagnostic_message(&close_message, "file:///tmp/closed.terl", 1);

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

#[test]
fn track_open_documents() {
    let store = OpenDocuments::default();
    let uri_one = Url::parse("file:///tmp/module_one.terl").expect("uri");
    let uri_two = Url::parse("file:///tmp/module_two.terl").expect("uri");

    assert!(!store.is_open(&uri_one));
    assert_eq!(store.count(), 0);

    store.open(
        uri_one.clone(),
        "module one.\n".to_string(),
        1,
        "terlan".to_string(),
    );
    assert!(store.is_open(&uri_one));
    assert_eq!(store.count(), 1);

    let first = store.snapshot(&uri_one).expect("first open");
    assert_eq!(first.version, 1);
    assert_eq!(first.text, "module one.\n");
    assert!(first.parse_ok);
    assert!(first.resolve_diagnostics.is_empty());
    assert!(first.type_diagnostics.is_empty());

    store.open(
        uri_one.clone(),
        "module one_updated.\n".to_string(),
        2,
        "terlan".to_string(),
    );
    let updated = store.snapshot(&uri_one).expect("updated");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.text, "module one_updated.\n");
    assert!(updated.parse_ok);
    assert!(updated.resolve_diagnostics.is_empty());
    assert!(updated.type_diagnostics.is_empty());

    store.open(
        uri_two.clone(),
        "module two.\n".to_string(),
        1,
        "terlan".to_string(),
    );
    assert_eq!(store.count(), 2);
    let second = store.snapshot(&uri_two).expect("uri two");
    assert!(second.parse_ok);
    assert!(second.resolve_diagnostics.is_empty());
    assert!(second.type_diagnostics.is_empty());

    assert!(store.close(&uri_one).is_some());
    assert!(!store.is_open(&uri_one));
    assert_eq!(store.count(), 1);
    assert!(store.snapshot(&uri_two).is_some());

    store.open(
        uri_two.clone(),
        "module broken".to_string(),
        2,
        "terlan".to_string(),
    );
    let broken_parse = store.snapshot(&uri_two).expect("broken parse");
    assert!(!broken_parse.parse_ok);
    assert!(broken_parse.resolve_diagnostics.is_empty());
    assert!(broken_parse.type_diagnostics.is_empty());

    store.open(
        uri_two.clone(),
        "module duplicate.\n\ntype A = ok.\ntype A = error.\n".to_string(),
        3,
        "terlan".to_string(),
    );
    let duplicate = store.snapshot(&uri_two).expect("duplicate resolve");
    assert!(duplicate.parse_ok);
    assert!(!duplicate.resolve_diagnostics.is_empty());
    assert!(!duplicate.type_diagnostics.is_empty());

    store.open(
        uri_two.clone(),
        "module type_error.\n\npub bad(X: Int): Binary ->\n    X + 1.\n".to_string(),
        4,
        "terlan".to_string(),
    );
    let type_error = store.snapshot(&uri_two).expect("type error");
    assert!(type_error.parse_ok);
    assert!(type_error.resolve_diagnostics.is_empty());
    assert!(!type_error.type_diagnostics.is_empty());
}

/// Verifies hostile Unicode syntax errors stay isolated to parser diagnostics.
///
/// Inputs:
/// - A source document with multibyte identifiers/text and an unterminated
///   expression.
///
/// Output:
/// - Test passes when LSP document state records a parser diagnostic without
///   resolver, typechecker, or template diagnostics.
///
/// Transformation:
/// - Exercises adversarial LSP document opening without JSON-RPC transport so
///   malformed Unicode-heavy source cannot cascade into later compiler stages.
#[test]
fn adversarial_lsp_diagnostics_isolate_unicode_parse_failures() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/adversarial_unicode.terl").expect("uri");
    let source = "\
module adversarial_unicode.

pub broken(): String ->
    \"λ🔥";

    let parse_error = store
        .open(uri.clone(), source.to_string(), 7, "terlan".to_string())
        .expect("malformed source should return parser diagnostic");
    let document = store.snapshot(&uri).expect("cached adversarial document");

    assert_eq!(document.version, 7);
    assert!(!document.parse_ok);
    assert!(!parse_error.message.trim().is_empty());
    assert!(parse_error.span.start <= source.len());
    assert!(parse_error.span.end <= source.len());
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document.type_diagnostics.is_empty());
    assert!(document.template_diagnostics.is_empty());
}

/// Verifies template documents are not parsed as Terlan source modules.
///
/// Inputs:
/// - One open document with a template language id and HTML-like body text.
///
/// Output:
/// - Test passes when the document is cached as a template with no parser,
///   resolver, or typechecker diagnostics.
///
/// Transformation:
/// - Locks the editor/LSP contract that `.terl.*` templates may attach to the
///   language server without receiving bogus module parse errors before
///   target-aware template diagnostics are implemented.
#[test]
fn open_template_document_skips_source_module_parsing() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/page.terl.html").expect("uri");

    let parse_error = store.open(
        uri.clone(),
        "<main>${title}</main>\n".to_string(),
        1,
        "terlan-template-html".to_string(),
    );

    assert!(parse_error.is_none());
    let document = store.snapshot(&uri).expect("template document");
    assert_eq!(document.language_id, "terlan-template-html");
    assert_eq!(document.kind, DocumentKind::Template);
    assert!(document.parse_ok);
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document.type_diagnostics.is_empty());
    assert!(document.template_diagnostics.is_empty());
    assert!(!document.is_source_like());
}

#[test]
fn open_document_loads_local_typi_interfaces_for_resolution() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-local-interfaces-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    fs::write(
        temp_dir.join("provider.typi"),
        "module provider.\n\npub type Item = ok.\n",
    )
    .expect("write interface");

    let store = OpenDocuments::default();
    let uri = Url::from_file_path(temp_dir.join("consumer.terl")).expect("file uri");
    store.open(
        uri.clone(),
        "module consumer.\n\nimport type provider.{Item}.\n".to_string(),
        1,
        "terlan".to_string(),
    );

    let consumer = store.snapshot(&uri).expect("consumer");
    assert!(consumer.parse_ok);
    assert!(consumer.resolve_diagnostics.is_empty());
    assert!(consumer.type_diagnostics.is_empty());

    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
}

#[test]
fn document_symbols_include_module_and_named_declarations() {
    let symbols = Backend::document_symbols_for_text(
        "\
module symbols.Main.

pub type UserId = Int.

pub struct User {
  id: UserId,
  name: String
}.

pub greet(user: User): String ->
  user.name.
",
    );

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "symbols.Main");
    assert_eq!(symbols[0].kind, SymbolKind::MODULE);
    assert_eq!(symbols[0].selection_range.start, Position::new(0, 7));
    assert_eq!(symbols[0].selection_range.end, Position::new(0, 19));
    let children = symbols[0].children.as_ref().expect("module children");
    let names = children
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            ("UserId", SymbolKind::TYPE_PARAMETER),
            ("User", SymbolKind::STRUCT),
            ("greet", SymbolKind::FUNCTION),
        ]
    );
    assert_eq!(children[0].selection_range.start, Position::new(2, 9));
    assert_eq!(children[0].selection_range.end, Position::new(2, 15));
    assert_eq!(children[1].selection_range.start, Position::new(4, 11));
    assert_eq!(children[1].selection_range.end, Position::new(4, 15));
    assert_eq!(children[2].selection_range.start, Position::new(9, 4));
    assert_eq!(children[2].selection_range.end, Position::new(9, 9));
}

#[test]
fn document_symbols_return_empty_for_parse_errors() {
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
fn definition_locations_resolve_same_document_function() {
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

/// Verifies identifier extraction stays conservative.
///
/// Inputs:
/// - A source snippet with identifiers and punctuation.
///
/// Output:
/// - Test passes when identifier offsets return names and punctuation offsets
///   return no result.
///
/// Transformation:
/// - Locks the ASCII identifier subset used by the current same-file
///   definition provider.
#[test]
fn identifier_at_byte_offset_extracts_ascii_identifier() {
    let text = "target().";

    assert_eq!(
        Backend::identifier_at_byte_offset(text, 2),
        Some("target".to_string())
    );
    assert_eq!(Backend::identifier_at_byte_offset(text, 7), None);
}

#[test]
fn open_document_position_to_byte_offset() {
    let doc = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "a😀\nxy".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 0)), Some(0));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 1)), Some(1));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 3)), Some(5));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 0)), Some(6));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 2)), Some(8));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 7)), None);
}

#[test]
fn open_document_position_to_byte_offset_with_crlf() {
    let doc = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "a😀\r\nb\n".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 0)), Some(0));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 1)), Some(1));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 3)), Some(5));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 0)), Some(7));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 1)), Some(8));
    assert_eq!(doc.byte_offset_from_position(Position::new(2, 0)), Some(9));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 2)), None);
}

#[test]
fn open_document_position_to_byte_offset_invalid_inputs() {
    let doc = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "hello\nworld".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    assert_eq!(doc.byte_offset_from_position(Position::new(5, 0)), None);
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 99)), None);
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 6)), None);
}

#[test]
fn open_document_range_from_span_uses_utf16_positions() {
    let text = "module emoji.\n\npub value(): Text ->\n    \"a😀b\".\n";
    let start = text.find('😀').expect("emoji offset");
    let end = start + '😀'.len_utf8();

    let range = OpenDocument::range_from_span(text, &Span::new(start, end));

    assert_eq!(range.start, Position::new(3, 6));
    assert_eq!(range.end, Position::new(3, 8));
}

#[test]
fn open_document_range_from_span_handles_crlf() {
    let text = "module crlf.\r\n\r\npub value(): Int ->\r\n    1.\r\n";
    let start = text.find('1').expect("number offset");
    let end = start + 1;

    let range = OpenDocument::range_from_span(text, &Span::new(start, end));

    assert_eq!(range.start, Position::new(3, 4));
    assert_eq!(range.end, Position::new(3, 5));
}
