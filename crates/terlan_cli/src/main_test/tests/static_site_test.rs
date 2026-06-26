use super::*;

#[test]
fn parse_serve_static_args_preserves_shared_server_settings() {
    let parsed = parse_serve_static_args(&[
        "src/site.terl".to_string(),
        "--host".to_string(),
        "0.0.0.0".to_string(),
        "--port".to_string(),
        "9010".to_string(),
        "--poll-ms".to_string(),
        "250".to_string(),
        "--source-dir".to_string(),
        "src".to_string(),
        "--validate-output".to_string(),
        "--base-path".to_string(),
        "/terlan".to_string(),
        "--check".to_string(),
    ])
    .expect("parse serve-static args");

    assert_eq!(parsed.file, "src/site.terl");
    assert_eq!(parsed.host, "0.0.0.0");
    assert_eq!(parsed.port, 9010);
    assert_eq!(parsed.poll_ms, 250);
    assert_eq!(parsed.source_dir, Some(PathBuf::from("src")));
    assert!(parsed.check_only);
    assert!(parsed.emit_args.validate_output);
    assert_eq!(parsed.emit_args.base_path.as_deref(), Some("/terlan/"));
}

/// Verifies `serve-static --check` renders once and exits without binding.
///
/// Inputs:
/// - A tiny static Terlan source file.
/// - `serve-static` command arguments with check-only validation enabled.
///
/// Output:
/// - Test passes when the command returns success and writes generated HTML.
///
/// Transformation:
/// - Exercises the same command path used by the dev server while proving CI
///   can validate static output without starting a long-running process.
#[test]
fn run_serve_static_check_renders_and_exits() {
    let dir = make_temp_dir("serve_static_check");
    let path = fixture(
        &dir,
        "module site.\n\npub page(): Html ->\n    html { <main>Check</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_serve_static(
        CliCommand {
            verb: Some("serve-static".into()),
            args: vec![path, "--validate-output".into(), "--check".into()],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
        "<main>Check</main>"
    );
}

/// Verifies public static emit accepts `--out-dir` in init next-step position.
///
/// Inputs:
/// - A tiny static Terlan source file.
/// - Top-level CLI args matching `terlc static emit <file> --out-dir <dir>`.
///
/// Output:
/// - Success exit code and generated HTML in the requested output directory.
///
/// Transformation:
/// - Exercises global option extraction across the public static command so
///   generated init guidance cannot drift from real CLI behavior.
#[test]
fn run_cli_static_emit_accepts_out_dir_after_source_path() {
    let dir = make_temp_dir("static_emit_out_dir_after_source");
    let path = fixture(
        &dir,
        "module site.\n\npub page(): Html ->\n    html { <main>Static</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_cli(vec![
        "static".to_string(),
        "emit".to_string(),
        path,
        "--out-dir".to_string(),
        out_dir.display().to_string(),
        "--validate-output".to_string(),
    ]);

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
        "<main>Static</main>"
    );
    fs::remove_dir_all(dir).expect("cleanup static emit fixture");
}

/// Verifies public static check accepts `--out-dir` in init next-step position.
///
/// Inputs:
/// - A tiny static Terlan source file.
/// - Top-level CLI args matching `terlc static check <file> --out-dir <dir>`.
///
/// Output:
/// - Success exit code and generated validation HTML in the requested output
///   directory.
///
/// Transformation:
/// - Exercises global option extraction across the public static check command
///   so CI-facing validation guidance cannot drift from real CLI behavior.
#[test]
fn run_cli_static_check_accepts_out_dir_after_source_path() {
    let dir = make_temp_dir("static_check_out_dir_after_source");
    let path = fixture(
        &dir,
        "module site.\n\npub page(): Html ->\n    html { <main>Check</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_cli(vec![
        "static".to_string(),
        "check".to_string(),
        path,
        "--out-dir".to_string(),
        out_dir.display().to_string(),
        "--base-path".to_string(),
        "/docs".to_string(),
    ]);

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
        "<base href=\"/docs/\"><main>Check</main>"
    );
    fs::remove_dir_all(dir).expect("cleanup static check fixture");
}

#[test]
fn formal_static_syntax_output_discovers_entrypoints_and_routes() {
    let module = parse_module_as_syntax_output(
        "\
module site.\n\
\n\
pub index(): Html ->\n\
    html { <main></main> }.\n\
\n\
static route \"/\" ->\n\
    home().\n\
\n\
home(): Html ->\n\
    html { <main><h1>Home</h1></main> }.\n\
",
    )
    .expect("parse syntax-output static module");

    assert_eq!(
        discover_syntax_static_entrypoints(&module),
        vec!["index".to_string()]
    );
    let routes = discover_syntax_static_routes(&module).expect("discover syntax routes");
    assert_eq!(
        routes,
        vec![StaticRoute {
            path: "/".to_string(),
            handler: "home".to_string(),
        }]
    );
    validate_syntax_static_route_handlers(&module, &routes)
        .expect("syntax route handlers should be valid");
}

/// Verifies static route parsing accepts compact function-call punctuation.
///
/// Inputs:
/// - A single-line `static route` declaration with `home().`.
///
/// Output:
/// - Test passes when the route parser extracts the path and handler.
///
/// Transformation:
/// - Exercises the static-route tokenizer without requiring whitespace around
///   the handler call punctuation.
#[test]
fn parse_static_routes_text_accepts_compact_singular_route() {
    let parsed = parse_static_routes_text(r#"static route "/guides/install" -> home()."#)
        .expect("parse compact static route");

    assert!(!parsed.is_block);
    assert_eq!(
        parsed.routes,
        vec![StaticRoute {
            path: "/guides/install".to_string(),
            handler: "home".to_string(),
        }]
    );
}

/// Verifies static route blocks accept compact repeated route entries.
///
/// Inputs:
/// - A `static routes` block with compact `handler().` entries.
///
/// Output:
/// - Test passes when both routes are parsed in declaration order.
///
/// Transformation:
/// - Exercises block route tokenization across braces, call punctuation, and
///   declaration terminators without relying on whitespace splitting.
#[test]
fn parse_static_routes_text_accepts_compact_route_block() {
    let parsed = parse_static_routes_text(
        r#"static routes { "/" -> home(). "/guides/install" -> install(). }."#,
    )
    .expect("parse compact static routes block");

    assert!(parsed.is_block);
    assert_eq!(
        parsed.routes,
        vec![
            StaticRoute {
                path: "/".to_string(),
                handler: "home".to_string(),
            },
            StaticRoute {
                path: "/guides/install".to_string(),
                handler: "install".to_string(),
            },
        ]
    );
}

#[test]
fn formal_static_emit_renders_html_blocks_from_syntax_output() {
    let dir = make_temp_dir("formal_static_emit");
    let path = fixture(
            &dir,
            "module site.\n\npub page(): Html ->\n    html { <main class=\"home\"><h1>Hello</h1></main> }.\n",
        );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
        "<main class=\"home\"><h1>Hello</h1></main>"
    );
}

/// Verifies static emit can write project-prefix-compatible HTML.
///
/// Inputs:
/// - A tiny static Terlan source file.
/// - `emit-static --base-path /terlan`.
///
/// Output:
/// - Test passes when generated HTML includes a deterministic base tag.
///
/// Transformation:
/// - Exercises the GitHub Pages project-prefix path without changing default
///   static output for users who do not supply `--base-path`.
#[test]
fn formal_static_emit_injects_base_path_when_requested() {
    let dir = make_temp_dir("formal_static_base_path");
    let path = fixture(
        &dir,
        "module site.\n\npub page(): Html ->\n    html { <main><a href=\"guides/install\">Install</a></main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path, "--base-path".into(), "/terlan".into()],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
        "<base href=\"/terlan/\"><main><a href=\"guides/install\">Install</a></main>"
    );
}

/// Verifies static route handlers accept the public template HTML type.
///
/// Inputs:
/// - A static Terlan source file importing `std.template.Template`.
/// - A route handler returning `Template.Html`.
///
/// Output:
/// - Test passes when static emit succeeds and writes generated HTML.
///
/// Transformation:
/// - Exercises the syntax-output route validator with the public std template
///   HTML type instead of the older internal `Html` spelling.
#[test]
fn formal_static_emit_accepts_template_html_route_return_type() {
    let dir = make_temp_dir("formal_static_template_html_route");
    let path = fixture(
        &dir,
        "module site.\n\nimport std.template.Template.\n\npub page(): Template.Html ->\n    html { <main>Template Html</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
        "<main>Template Html</main>"
    );
}

/// Verifies static emit copies structurally valid JSON artifact-template assets.
///
/// Inputs:
/// - A static Terlan source file importing `data.terl.json` as a file asset.
/// - A JSON artifact template containing a string interpolation island.
///
/// Output:
/// - Test passes when static emit succeeds and the template asset is copied to
///   the output directory unchanged.
///
/// Transformation:
/// - Exercises the public `emit-static` command path so artifact-template
///   validation is covered where static assets are packaged.
#[test]
fn formal_static_emit_copies_valid_json_artifact_template_asset() {
    let dir = make_temp_dir("formal_static_json_artifact");
    fs::create_dir_all(dir.join("assets")).expect("create assets");
    fs::write(
        dir.join("assets/data.terl.json"),
        "{ \"title\": \"Hello ${name}\", \"count\": 1 }\n",
    )
    .expect("write json artifact template");
    let path = fixture(
        &dir,
        "module site.\n\nimport file \"./assets/data.terl.json\" as Data.\n\npub page(): Html ->\n    html { <main>JSON</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("data.terl.json")).expect("read copied json artifact"),
        "{ \"title\": \"Hello ${name}\", \"count\": 1 }\n"
    );
}

/// Verifies static emit rejects malformed JSON artifact-template imports.
///
/// Inputs:
/// - A static Terlan source file importing `data.terl.json` as a file asset.
/// - A malformed JSON artifact template.
///
/// Output:
/// - Test passes when static emit exits with failure.
///
/// Transformation:
/// - Confirms accepted static assets go through target-aware artifact-template
///   validation before they are copied.
#[test]
fn formal_static_emit_rejects_invalid_json_artifact_template_asset() {
    let dir = make_temp_dir("formal_static_invalid_json_artifact");
    fs::create_dir_all(dir.join("assets")).expect("create assets");
    fs::write(dir.join("assets/data.terl.json"), "{ \"title\": }\n")
        .expect("write invalid json artifact template");
    let path = fixture(
        &dir,
        "module site.\n\nimport file \"./assets/data.terl.json\" as Data.\n\npub page(): Html ->\n    html { <main>JSON</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::from(1));
}

#[test]
fn formal_static_emit_renders_markdown_html_from_syntax_output() {
    let dir = make_temp_dir("formal_static_markdown");
    fs::create_dir_all(dir.join("posts")).expect("create posts");
    fs::write(
        dir.join("posts/welcome.md"),
        "# Welcome\n\nThis page came from **Markdown**.\n",
    )
    .expect("write markdown");
    let path = fixture(
            &dir,
            "module site.\n\nimport markdown \"./posts/welcome.md\" as WelcomePost.\n\npub post(): Html ->\n    WelcomePost.html.\n",
        );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let html = fs::read_to_string(out_dir.join("post.html")).expect("read markdown html");
    assert!(html.contains("<h1>Welcome</h1>"));
    assert!(html.contains("<strong>Markdown</strong>"));
}

/// Verifies static emit writes discovered Markdown content routes.
///
/// Inputs:
/// - A static Terlan source file importing a `content/` Markdown page.
/// - Markdown content with `@page.route` metadata.
///
/// Output:
/// - Test passes when `emit-static` writes the Markdown page at the discovered
///   route output path.
///
/// Transformation:
/// - Exercises metadata-preserving Markdown collection, route discovery, and
///   static file emission through the public command path.
#[test]
fn formal_static_emit_writes_markdown_content_routes() {
    let dir = make_temp_dir("formal_static_markdown_routes");
    fs::create_dir_all(dir.join("content/guides")).expect("create content");
    fs::write(
        dir.join("content/guides/install.terl.md"),
        "@page { title = \"Install\", route = \"/install\" }\n\n# Install\n\nRun `terlc`.\n",
    )
    .expect("write Markdown content");
    let path = fixture(
        &dir,
        "module site.\n\nimport markdown \"./content/guides/install.terl.md\" as Install.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let html =
        fs::read_to_string(out_dir.join("install/index.html")).expect("read Markdown route html");
    assert!(html.contains("<h1>Install</h1>"));
    assert!(html.contains("<code>terlc</code>"));
}

/// Verifies static emit renders Markdown content through a page layout.
///
/// Inputs:
/// - A static Terlan source file with a layout template declaration.
/// - Markdown content declaring `@page.layout` and `@page.title`.
///
/// Output:
/// - Test passes when the emitted route wraps Markdown HTML in the declared
///   layout template.
///
/// Transformation:
/// - Exercises the static renderer path that maps Markdown page metadata to
///   layout values: `${title}` as text and `${children}` as rendered Markdown
///   HTML.
#[test]
fn formal_static_emit_renders_markdown_content_layout() {
    let dir = make_temp_dir("formal_static_markdown_layout");
    fs::create_dir_all(dir.join("content")).expect("create content");
    fs::create_dir_all(dir.join("templates")).expect("create templates");
    fs::write(
        dir.join("templates/page.terl.html"),
        "<main><h1>${title}</h1><section>${children}</section></main>",
    )
    .expect("write layout template");
    fs::write(
        dir.join("content/install.terl.md"),
        "@page { title = \"Install\", layout = \"PageLayout\" }\n\n## Steps\n\nRun `terlc`.\n",
    )
    .expect("write Markdown content");
    let path = fixture(
        &dir,
        "module site.\n\ntemplate PageLayout from \"./templates/page.terl.html\" {\n    title: Text\n}.\n\nimport markdown \"./content/install.terl.md\" as Install.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let html =
        fs::read_to_string(out_dir.join("install/index.html")).expect("read layout route html");
    assert_eq!(
        html,
        "<main><h1>Install</h1><section><h2>Steps</h2>\n<p>Run <code>terlc</code>.</p>\n</section></main>"
    );
}

/// Verifies static emit rejects Markdown routes that collide with handler routes.
///
/// Inputs:
/// - A static Terlan source file with an explicit `/install` route.
/// - A Markdown import whose metadata also declares `/install`.
///
/// Output:
/// - Test passes when `emit-static` exits with an error before writing
///   colliding route outputs.
///
/// Transformation:
/// - Confirms explicit routes and content routes share one URL namespace.
#[test]
fn formal_static_emit_rejects_markdown_route_collisions() {
    let dir = make_temp_dir("formal_static_markdown_route_collision");
    fs::create_dir_all(dir.join("content")).expect("create content");
    fs::write(
        dir.join("content/install.terl.md"),
        "@page { route = \"/install\" }\n\n# Install\n",
    )
    .expect("write Markdown content");
    let path = fixture(
        &dir,
        "module site.\n\nimport markdown \"./content/install.terl.md\" as Install.\n\nstatic route \"/install\" -> install().\n\ninstall(): Html ->\n    html { <main>Install</main> }.\n",
    );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::from(1));
}

#[test]
fn formal_static_emit_renders_external_template_from_syntax_output() {
    let dir = make_temp_dir("formal_static_template");
    fs::create_dir_all(dir.join("templates")).expect("create templates");
    fs::write(
        dir.join("templates/card.terl.html"),
        "<article data-id=\"${user.id}\"><h1>${title}</h1><p>${user.name}</p></article>",
    )
    .expect("write template");
    let path = fixture(
            &dir,
            "module site.\n\nimport std.template.Template.\n\npub struct User {\n    id: Int,\n    name: Text\n}.\n\ntemplate Card from \"./templates/card.terl.html\" {\n    title: Text,\n    user: User\n}.\n\npub page(): Template.Html ->\n    Card{ title = \"Hi & Bye\", user = User(id = 7, name = \"Ada <A>\") }.\n",
        );
    let out_dir = dir.join("public");

    let exit = run_emit_static(
        CliCommand {
            verb: Some("emit-static".into()),
            args: vec![path],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(out_dir.join("page.html")).expect("read template html"),
        "<article data-id=\"7\"><h1>Hi&#32;&amp;&#32;Bye</h1><p>Ada&#32;&lt;A&gt;</p></article>"
    );
}

#[test]
fn formal_static_emit_renders_external_template_components_from_syntax_output() {
    let dir = make_temp_dir("formal_static_template_component");
    fs::create_dir_all(dir.join("templates")).expect("create templates");
    fs::write(
        dir.join("templates/page_shell.terl.html"),
        "<main class=\"${shell_class}\">${children}</main>",
    )
    .expect("write shell template");
    fs::write(
        dir.join("templates/page.terl.html"),
        "<page-shell shell_class=\"shell\"><h1>${title}</h1><p>Wrapped</p></page-shell>",
    )
    .expect("write page template");
    let source = "module site.\n\ntemplate PageShell from \"./templates/page_shell.terl.html\" {\n    shell_class: Text\n}.\n\ntemplate Page from \"./templates/page.terl.html\" {\n    title: Text\n}.\n\npub home(): Html ->\n    Page{ title = \"Home\" }.\n";
    let path = fixture(&dir, source);
    let module = parse_module_as_syntax_output(source).expect("parse syntax-output module");
    let templates = commands::artifacts::collect_syntax_template_inputs(&module, Path::new(&path))
        .expect("collect templates");

    let html = commands::static_site::render_syntax_static_entrypoint(
        &module,
        &templates,
        &BTreeMap::new(),
        "home",
    )
    .expect("render syntax static template component");

    assert_eq!(
        html,
        "<main class=\"shell\"><h1>Home</h1><p>Wrapped</p></main>"
    );
}

#[test]
fn formal_static_emit_renders_inline_template_components_from_syntax_output() {
    let dir = make_temp_dir("formal_static_inline_template_component");
    fs::create_dir_all(dir.join("templates")).expect("create templates");
    fs::write(
        dir.join("templates/page_shell.terl.html"),
        "<main class=\"${shell_class}\">${view1}<span>and</span>${view2}${children}</main>",
    )
    .expect("write shell template");
    fs::write(
        dir.join("templates/welcome_content.terl.html"),
        "<p>Welcome</p>",
    )
    .expect("write welcome template");
    let source = "module site.\n\ntemplate PageShell from \"./templates/page_shell.terl.html\" {\n    shell_class: Text,\n    view1: Html,\n    view2: Html\n}.\n\ntemplate WelcomeContent from \"./templates/welcome_content.terl.html\" {}.\n\npub home(): Html ->\n    html {\n        <page-shell shell_class=\"shell\">\n            @view1 {\n                <welcome-content></welcome-content>\n            }\n            @view2 {\n                <p>Second</p>\n            }\n            <p>After</p>\n        </page-shell>\n    }.\n";
    let path = fixture(&dir, source);
    let module = parse_module_as_syntax_output(source).expect("parse syntax-output module");
    let templates = commands::artifacts::collect_syntax_template_inputs(&module, Path::new(&path))
        .expect("collect templates");

    let html = commands::static_site::render_syntax_static_entrypoint(
        &module,
        &templates,
        &BTreeMap::new(),
        "home",
    )
    .expect("render syntax inline static template component");

    assert_eq!(
        html,
        "<main class=\"shell\"><p>Welcome</p><span>and</span><p>Second</p><p>After</p></main>"
    );
}
