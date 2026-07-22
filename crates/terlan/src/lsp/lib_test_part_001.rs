use super::document::{DocumentKind, OpenDocument, OpenDocuments};
use super::Backend;
use crate::terlan_syntax::ebnf::EbnfSourceSpan;
use crate::terlan_syntax::{format_source_module, Span};
use crate::terlan_syntax::{SyntaxParamOutput, SyntaxTypeOutput};
use std::fs;
use std::io::{self as std_io, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::time::{timeout, Duration};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, Position, SymbolKind, Url,
};
use tower_lsp::{LspService, Server};

#[test]
fn value_lifecycle_semantic_tokens_mark_constants_read_only() {
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "module semantic.constants.\npub const ANSWER: Int = 42.\npub answer(): Int -> ANSWER.\n".to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    let tokens = Backend::value_lifecycle_semantic_tokens(&document);
    assert_eq!(tokens.data.len(), 3);
    assert_eq!(tokens.data[0].token_type, 0);
    assert_eq!(tokens.data[1].token_type, 1);
    assert_eq!(tokens.data[1].token_modifiers_bitset, 1);
    assert_eq!(tokens.data[2].token_type, 1);
    assert_eq!(tokens.data[2].token_modifiers_bitset, 1);
}

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
    assert!(initialize_response.contains(r#""declarationProvider":true"#));
    assert!(initialize_response.contains(r#""hoverProvider":true"#));
    assert!(initialize_response.contains(r#""signatureHelpProvider""#));
    assert!(initialize_response.contains(r#""inlayHintProvider":true"#));
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
