
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

/// Verifies LSP document diagnostics accept string capture patterns.
///
/// Inputs:
/// - A source document with `${name}` and `${name: Type}` captures in a guarded
///   case pattern.
///
/// Output:
/// - Test passes when the document parses, resolves, and typechecks without
///   diagnostics.
///
/// Transformation:
/// - Runs the same open-document pipeline used by LSP publish diagnostics so
///   editor buffers stay aligned with the compiler string-pattern surface.
#[test]
fn lsp_document_accepts_string_capture_patterns() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/string_capture_lsp.terl").expect("uri");
    let source = r#"
module string_capture_lsp.

pub route(path: String): String ->
    case path {
        "users/${id: Int}/${name}.json" where id > 0 -> name;
        _ -> "missing"
    }.
"#;

    let parse_error = store.open(uri.clone(), source.to_string(), 1, "terlan".to_string());
    let document = store
        .snapshot(&uri)
        .expect("cached string capture document");

    assert!(
        parse_error.is_none(),
        "unexpected parse error: {parse_error:?}"
    );
    assert!(document.parse_ok);
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document.type_diagnostics.is_empty());
    assert!(document.template_diagnostics.is_empty());
}

/// Verifies LSP diagnostics reject ambiguous string capture patterns.
///
/// Inputs:
/// - A source document with adjacent `${...}` captures and no literal separator.
///
/// Output:
/// - Test passes when the parser diagnostic is cached without cascading into
///   resolver or typechecker diagnostics.
///
/// Transformation:
/// - Locks the editor diagnostic path to the same deterministic matching rule
///   enforced by `terlc`.
#[test]
fn lsp_document_rejects_adjacent_string_capture_patterns() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/string_capture_lsp_bad.terl").expect("uri");
    let source = r#"
module string_capture_lsp_bad.

pub route(path: String): String ->
    case path {
        "${prefix}${suffix}" -> prefix;
        _ -> "missing"
    }.
"#;

    let parse_error = store
        .open(uri.clone(), source.to_string(), 1, "terlan".to_string())
        .expect("adjacent captures should produce a parser diagnostic");
    let document = store
        .snapshot(&uri)
        .expect("cached invalid string capture document");

    assert_eq!(
        parse_error.message,
        "adjacent string captures require a literal separator"
    );
    assert!(!document.parse_ok);
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document.type_diagnostics.is_empty());
    assert!(document.template_diagnostics.is_empty());
}

/// Verifies LSP diagnostics parse shape declarations and report the semantic
/// declaration support.
///
/// Inputs:
/// - A source document with a public string-capture shape and a guarded
///   structural shape.
///
/// Output:
/// - Test passes when the document has no parse, resolve, or type diagnostics.
///
/// Transformation:
/// - Runs the same open-document pipeline used by editor diagnostics so shape
///   syntax remains available to tooling independently of whether a guarded
///   alias is invoked in executable code.
#[test]
fn lsp_document_accepts_shape_synonym_declarations() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/shape_synonym_lsp.terl").expect("uri");
    let source = r#"
module shape_synonym_lsp.

pub shape UserAsset(id, file) =
    "users/${id: Int}/assets/${file}".

shape OkResponse(body) =
    {status, body} where status in 200..299.
"#;

    let parse_error = store.open(uri.clone(), source.to_string(), 1, "terlan".to_string());
    let document = store.snapshot(&uri).expect("cached shape synonym document");

    assert!(
        parse_error.is_none(),
        "unexpected parse error: {parse_error:?}"
    );
    assert!(document.parse_ok);
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

/// Verifies editor diagnostics use trait-backed receiver dispatch.
///
/// Inputs:
/// - An opened Terlan document containing an explicit trait implementation.
/// - A function that calls the trait method with receiver syntax.
///
/// Output:
/// - The LSP document snapshot has no parse, resolve, or type diagnostics.
///
/// Transformation:
/// - Runs the same parse/resolve/typecheck path used for editor diagnostics so
///   `value.method()` highlighting/underlines stay aligned with compiler
///   trait dispatch.
#[test]
fn open_document_accepts_trait_backed_receiver_method_call() {
    let store = OpenDocuments::default();
    let uri = Url::parse("file:///tmp/trait-receiver.terl").expect("uri");
    store.open(
        uri.clone(),
        "\
module trait_receiver_editor.\n\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[User] for User {\n\
    show(value: User): String ->\n\
        value.name.\n\
}.\n\
\n\
pub describe(value: User): String ->\n\
    value.show().\n\
"
        .to_string(),
        1,
        "terlan".to_string(),
    );

    let document = store.snapshot(&uri).expect("document");
    assert!(document.parse_ok);
    assert!(document.resolve_diagnostics.is_empty());
    assert!(document.type_diagnostics.is_empty());
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
            ("UserId", SymbolKind::STRUCT),
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

/// Verifies guarded function clauses remain visible in editor Outline.
///
/// Inputs:
/// - A parseable module with a multi-clause function using a `where` guard.
///
/// Output:
/// - Test success when LSP document symbols include the guarded function as a
///   normal function declaration.
///
/// Transformation:
/// - Exercises compiler-backed syntax output through the LSP projection layer
///   so guard syntax cannot regress editor navigation.
#[test]
fn document_symbols_include_guarded_function_clauses() {
    let symbols = Backend::document_symbols_for_text(
        "\
module symbols.Guards.

pub classify(value) where value < 0 ->
  \"negative\";
classify(_value) ->
  \"positive\".
",
    );

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "symbols.Guards");
    let children = symbols[0].children.as_ref().expect("module children");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "classify");
    assert_eq!(children[0].detail.as_deref(), Some("pub function"));
    assert_eq!(children[0].kind, SymbolKind::FUNCTION);
    assert_eq!(children[0].selection_range.start, Position::new(2, 4));
    assert_eq!(children[0].selection_range.end, Position::new(2, 12));
}

/// Verifies parse-preserved shape declarations remain visible in editor Outline.
///
/// Inputs:
/// - A parseable module with public and private raw `shape` declarations.
///
/// Output:
/// - Test success when LSP document symbols expose both shapes with stable
///   visibility detail and selection ranges.
///
/// Transformation:
/// - Exercises the compiler-backed syntax output through the LSP projection
///   layer while shape expansion is still intentionally blocked downstream.
#[test]
fn document_symbols_include_raw_shape_declarations() {
    let symbols = Backend::document_symbols_for_text(
        "\
module symbols.Shapes.

pub shape UserAsset(id, file) =
  \"users/${id: Int}/assets/${file}\".

shape OkResponse(body) =
  {status, body} where status in 200..299.
",
    );

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "symbols.Shapes");
    let children = symbols[0].children.as_ref().expect("module children");
    let summary = children
        .iter()
        .map(|symbol| {
            (
                symbol.name.as_str(),
                symbol.detail.as_deref(),
                symbol.kind,
                symbol.selection_range.start,
                symbol.selection_range.end,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![
            (
                "UserAsset",
                Some("pub shape"),
                SymbolKind::STRUCT,
                Position::new(2, 10),
                Position::new(2, 19),
            ),
            (
                "OkResponse",
                Some("shape"),
                SymbolKind::STRUCT,
                Position::new(5, 6),
                Position::new(5, 16),
            ),
        ]
    );
}

/// Verifies completion uses the latest changed document contents.
///
/// Inputs:
/// - In-memory LSP client/server duplex streams.
/// - A document opened with one local binding and then changed to a different
///   local binding name.
///
/// Output:
/// - Test success when `textDocument/completion` returns the changed binding
///   and does not leak the stale binding from the previous version.
///
/// Transformation:
/// - Exercises the real didOpen, didChange, diagnostics, and completion
///   JSON-RPC path so editor completions cannot read stale buffer snapshots.
#[tokio::test]
async fn completion_request_uses_latest_changed_document_version() -> std_io::Result<()> {
    let uri = "file:///tmp/stale-completion.terl";
    let old_source = "\
module stale_completion.

pub caller(): Int ->
    let old_value = 1;
    old_value.
";
    let new_source = "\
module stale_completion.

pub caller(): Int ->
    let new_value = 1;
    new_value.
";
    let position_after = |source: &str, needle: &str| {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("missing completion marker {needle:?}"))
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
    let new_position = position_after(new_source, "let new_value = 1;\n    ");

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
                    "text": old_source
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
    assert_clear_diagnostic_message(&open_message, uri, 1);

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": new_source }]
            }
        })
        .to_string(),
    )
    .await?;
    let change_message = timeout(
        Duration::from_millis(500),
        read_lsp_message(&mut client_stdout),
    )
    .await
    .map_err(|_| std_io::Error::new(ErrorKind::TimedOut, "change diagnostics timeout"))??;
    assert_clear_diagnostic_message(&change_message, uri, 2);

    write_lsp_message(
        &mut client_to_server,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": {
                    "line": new_position.line,
                    "character": new_position.character
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
        completion_response.contains(r#""label":"new_value""#),
        "{completion_response}"
    );
    assert!(
        !completion_response.contains(r#""label":"old_value""#),
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
