use super::*;

#[test]
fn formal_static_syntax_output_discovers_entrypoints_and_routes() {
    let module = parse_module_as_syntax_output(
        "\
module site.\n\
\n\
pub index(): Html[Never] ->\n\
    html { <main></main> }.\n\
\n\
static route \"/\" ->\n\
    home().\n\
\n\
home(): Html[Never] ->\n\
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

#[test]
fn formal_static_emit_renders_html_blocks_from_syntax_output() {
    let dir = make_temp_dir("formal_static_emit");
    let path = fixture(
            &dir,
            "module site.\n\npub page(): Html[Never] ->\n    html { <main class=\"home\"><h1>Hello</h1></main> }.\n",
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
            "module site.\n\nimport markdown \"./posts/welcome.md\" as WelcomePost.\n\npub post(): Html[Never] ->\n    WelcomePost.html.\n",
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

#[test]
fn formal_static_emit_renders_external_template_from_syntax_output() {
    let dir = make_temp_dir("formal_static_template");
    fs::create_dir_all(dir.join("templates")).expect("create templates");
    fs::write(
        dir.join("templates/card.terl.html"),
        "<article data-id=\"{user.id}\"><h1>{title}</h1><p>{user.name}</p></article>",
    )
    .expect("write template");
    let path = fixture(
            &dir,
            "module site.\n\npub struct User {\n    id: Int,\n    name: Text\n}.\n\ntemplate Card from \"./templates/card.terl.html\" {\n    title: Text,\n    user: User\n}.\n\npub home(): Html[Never] ->\n    Card{ title = <<\"Hi & Bye\">>, user = #User{id = 7, name = <<\"Ada <A>\">>} }.\n",
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
        fs::read_to_string(out_dir.join("home.html")).expect("read template html"),
        "<article data-id=\"7\"><h1>Hi &amp; Bye</h1><p>Ada &lt;A&gt;</p></article>"
    );
}

#[test]
fn formal_static_emit_renders_external_template_components_from_syntax_output() {
    let dir = make_temp_dir("formal_static_template_component");
    fs::create_dir_all(dir.join("templates")).expect("create templates");
    fs::write(
        dir.join("templates/page_shell.terl.html"),
        "<main class=\"{shell_class}\">{children}</main>",
    )
    .expect("write shell template");
    fs::write(
        dir.join("templates/page.terl.html"),
        "<page-shell shell_class=\"shell\"><h1>{title}</h1><p>Wrapped</p></page-shell>",
    )
    .expect("write page template");
    let source = "module site.\n\ntemplate PageShell from \"./templates/page_shell.terl.html\" {\n    shell_class: Text\n}.\n\ntemplate Page from \"./templates/page.terl.html\" {\n    title: Text\n}.\n\npub home(): Html[Never] ->\n    Page{ title = \"Home\" }.\n";
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
        "<main class=\"{shell_class}\">{view1}<span>and</span>{view2}{children}</main>",
    )
    .expect("write shell template");
    fs::write(
        dir.join("templates/welcome_content.terl.html"),
        "<p>Welcome</p>",
    )
    .expect("write welcome template");
    let source = "module site.\n\ntemplate PageShell from \"./templates/page_shell.terl.html\" {\n    shell_class: Text,\n    view1: Html[Never],\n    view2: Html[Never]\n}.\n\ntemplate WelcomeContent from \"./templates/welcome_content.terl.html\" {}.\n\npub home(): Html[Never] ->\n    html {\n        <page-shell shell_class=\"shell\">\n            @view1 {\n                <welcome-content></welcome-content>\n            }\n            @view2 {\n                <p>Second</p>\n            }\n            <p>After</p>\n        </page-shell>\n    }.\n";
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
