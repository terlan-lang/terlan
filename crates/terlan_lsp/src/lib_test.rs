use super::Backend;
use super::OpenDocument;
use super::OpenDocuments;
use std::fs;
use std::io::{self as std_io, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};
use terlan_syntax::Span;
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::time::{timeout, Duration};
use tower_lsp::lsp_types::{Position, Url};
use tower_lsp::{LspService, Server};

async fn write_lsp_message(writer: &mut DuplexStream, payload: &str) -> std_io::Result<()> {
    let mut out = Vec::with_capacity(payload.len() + 64);
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
    out.extend_from_slice(payload.as_bytes());
    writer.write_all(&out).await?;
    writer.flush().await?;
    Ok(())
}

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

fn assert_parse_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":1"#));
    assert!(message.contains(r#""source":"terlan-syntax""#));
}

fn assert_type_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":1"#));
    assert!(message.contains(r#""source":"terlan-typeck""#));
}

fn assert_type_warning_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[{"#));
    assert!(message.contains(r#""severity":2"#));
    assert!(message.contains(r#""source":"terlan-typeck""#));
}

fn assert_clear_diagnostic_message(message: &str, uri: &str, version: i32) {
    assert!(message.contains(r#""method":"textDocument/publishDiagnostics""#));
    assert!(message.contains(&format!(r#""uri":"{uri}""#)));
    assert!(message.contains(&format!(r#""version":{version}"#)));
    assert!(message.contains(r#""diagnostics":[]"#));
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
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///tmp/closed.terl","languageId":"terlan","version":1,"text":"module closed.\n"}}}"#,
        )
        .await?;
    let open_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "open diagnostics timeout"))??;
    assert_clear_diagnostic_message(&open_message, "file:///tmp/closed.terl", 1);

    write_lsp_message(
            &mut client_to_server,
            r#"{"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":"file:///tmp/closed.terl"}}}"#,
        )
        .await?;

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

    store.open(uri_one.clone(), "module one.\n".to_string(), 1);
    assert!(store.is_open(&uri_one));
    assert_eq!(store.count(), 1);

    let first = store.snapshot(&uri_one).expect("first open");
    assert_eq!(first.version, 1);
    assert_eq!(first.text, "module one.\n");
    assert!(first.parse_ok);
    assert!(first.resolve_diagnostics.is_empty());
    assert!(first.type_diagnostics.is_empty());

    store.open(uri_one.clone(), "module one_updated.\n".to_string(), 2);
    let updated = store.snapshot(&uri_one).expect("updated");
    assert_eq!(updated.version, 2);
    assert_eq!(updated.text, "module one_updated.\n");
    assert!(updated.parse_ok);
    assert!(updated.resolve_diagnostics.is_empty());
    assert!(updated.type_diagnostics.is_empty());

    store.open(uri_two.clone(), "module two.\n".to_string(), 1);
    assert_eq!(store.count(), 2);
    let second = store.snapshot(&uri_two).expect("uri two");
    assert!(second.parse_ok);
    assert!(second.resolve_diagnostics.is_empty());
    assert!(second.type_diagnostics.is_empty());

    assert!(store.close(&uri_one).is_some());
    assert!(!store.is_open(&uri_one));
    assert_eq!(store.count(), 1);
    assert!(store.snapshot(&uri_two).is_some());

    store.open(uri_two.clone(), "module broken".to_string(), 2);
    let broken_parse = store.snapshot(&uri_two).expect("broken parse");
    assert!(!broken_parse.parse_ok);
    assert!(broken_parse.resolve_diagnostics.is_empty());
    assert!(broken_parse.type_diagnostics.is_empty());

    store.open(
        uri_two.clone(),
        "module duplicate.\n\ntype A = ok.\ntype A = error.\n".to_string(),
        3,
    );
    let duplicate = store.snapshot(&uri_two).expect("duplicate resolve");
    assert!(duplicate.parse_ok);
    assert!(!duplicate.resolve_diagnostics.is_empty());
    assert!(!duplicate.type_diagnostics.is_empty());

    store.open(
        uri_two.clone(),
        "module type_error.\n\npub bad(X: Int): Binary ->\n    X + 1.\n".to_string(),
        4,
    );
    let type_error = store.snapshot(&uri_two).expect("type error");
    assert!(type_error.parse_ok);
    assert!(type_error.resolve_diagnostics.is_empty());
    assert!(!type_error.type_diagnostics.is_empty());
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
    );

    let consumer = store.snapshot(&uri).expect("consumer");
    assert!(consumer.parse_ok);
    assert!(consumer.resolve_diagnostics.is_empty());
    assert!(consumer.type_diagnostics.is_empty());

    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
}

#[test]
fn open_document_position_to_byte_offset() {
    let doc = OpenDocument {
        version: 1,
        text: "a😀\nxy".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
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
        text: "a😀\r\nb\n".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
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
        text: "hello\nworld".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
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
