use std::path::PathBuf;

use crate::commands::artifacts::SyntaxMarkdownInput;

use super::*;

/// Builds a Markdown frontend input for route-discovery tests.
///
/// Inputs:
/// - `alias`: import alias assigned by the Terlan source file.
/// - `source_path`: Markdown import path text.
/// - `metadata`: static page metadata extracted from the Markdown header.
///
/// Output:
/// - Minimal `SyntaxMarkdownInput` with empty rendered document content.
///
/// Transformation:
/// - Supplies only route-discovery fields while leaving the parsed document body
///   empty because route tests do not render Markdown.
fn markdown_input(
    alias: &str,
    source_path: &str,
    metadata: crate::terlan_html::PageMetadata,
) -> SyntaxMarkdownInput {
    SyntaxMarkdownInput {
        alias: alias.to_string(),
        source_path: source_path.to_string(),
        resolved_path: PathBuf::from(source_path),
        metadata,
        document: crate::terlan_html::MarkdownDocument {
            source_path: Some(PathBuf::from(source_path)),
            raw_source: String::new(),
            rendered_html: String::new(),
            nodes: Vec::new(),
            headings: Vec::new(),
        },
    }
}

/// Builds a Markdown frontend input with parsed document content.
///
/// Inputs:
/// - `alias`: import alias assigned by the Terlan source file.
/// - `source_path`: Markdown import path text.
/// - `metadata`: static page metadata extracted from the Markdown header.
/// - `body`: Markdown body to parse.
///
/// Output:
/// - `SyntaxMarkdownInput` whose document carries rendered Markdown nodes.
///
/// Transformation:
/// - Parses `body` through `crate::terlan_html::parse_markdown` so route-discovery
///   tests exercise the same heading extraction path as static rendering.
fn markdown_input_with_body(
    alias: &str,
    source_path: &str,
    metadata: crate::terlan_html::PageMetadata,
    body: &str,
) -> SyntaxMarkdownInput {
    let mut input = markdown_input(alias, source_path, metadata);
    input.document =
        crate::terlan_html::parse_markdown(body, source_path).expect("parse Markdown body");
    input
}

/// Verifies content paths infer nested static routes.
///
/// Inputs:
/// - A Markdown import path under `content/`.
///
/// Output:
/// - Test passes when route discovery maps the path to a URL path.
///
/// Transformation:
/// - Removes `content/` and `.terl.md`, then prefixes the remaining path with
///   `/`.
#[test]
fn markdown_static_routes_infer_nested_content_path() {
    let routes = discover_markdown_static_routes(&[markdown_input(
        "Install",
        "content/guides/install.terl.md",
        crate::terlan_html::PageMetadata::default(),
    )])
    .expect("discover Markdown static routes");

    assert_eq!(
        routes,
        vec![StaticMarkdownRoute {
            path: "/guides/install".to_string(),
            aliases: Vec::new(),
            alias: "Install".to_string(),
            title: None,
            layout: None,
        }]
    );
}

/// Verifies `index` Markdown files map to directory routes.
///
/// Inputs:
/// - Root and nested `index.terl.md` content paths.
///
/// Output:
/// - Test passes when root index maps to `/` and nested index maps to the
///   containing route.
///
/// Transformation:
/// - Drops the final `index` route segment after suffix stripping.
#[test]
fn markdown_static_routes_infer_index_content_paths() {
    let routes = discover_markdown_static_routes(&[
        markdown_input(
            "Home",
            "content/index.terl.md",
            crate::terlan_html::PageMetadata::default(),
        ),
        markdown_input(
            "Guides",
            "content/guides/index.terl.md",
            crate::terlan_html::PageMetadata::default(),
        ),
    ])
    .expect("discover index Markdown static routes");

    assert_eq!(
        routes,
        vec![
            StaticMarkdownRoute {
                path: "/".to_string(),
                aliases: Vec::new(),
                alias: "Home".to_string(),
                title: None,
                layout: None,
            },
            StaticMarkdownRoute {
                path: "/guides".to_string(),
                aliases: Vec::new(),
                alias: "Guides".to_string(),
                title: None,
                layout: None,
            },
        ]
    );
}

/// Verifies generated static-profile relative imports infer content routes.
///
/// Inputs:
/// - A Markdown import shaped like `../../content/index.terl.md`.
/// - A resolved path under the project `content/` directory.
///
/// Output:
/// - Test passes when route discovery maps the resolved content path to `/`.
///
/// Transformation:
/// - Falls back from raw import text to resolved path routing so generated
///   static projects can be emitted from any current working directory.
#[test]
fn markdown_static_routes_infer_generated_relative_content_imports() {
    let mut input = markdown_input(
        "Home",
        "../../content/index.terl.md",
        crate::terlan_html::PageMetadata::default(),
    );
    input.resolved_path = PathBuf::from("/tmp/site/content/index.terl.md");

    let routes = discover_markdown_static_routes(&[input])
        .expect("discover generated static Markdown route");

    assert_eq!(
        routes,
        vec![StaticMarkdownRoute {
            path: "/".to_string(),
            aliases: Vec::new(),
            alias: "Home".to_string(),
            title: None,
            layout: None,
        }]
    );
}

/// Verifies Markdown routes default the title from the first heading.
///
/// Inputs:
/// - A Markdown import without `@page.title`.
/// - Parsed Markdown content beginning with an H1.
///
/// Output:
/// - Static Markdown route whose title is the heading text.
///
/// Transformation:
/// - Reuses parsed Markdown HTML nodes so title defaults do not require a
///   second Markdown parser in route discovery.
#[test]
fn markdown_static_routes_default_title_from_first_heading() {
    let routes = discover_markdown_static_routes(&[markdown_input_with_body(
        "Install",
        "content/install.terl.md",
        crate::terlan_html::PageMetadata::default(),
        "# Install Terlan\n\nRun `terlc`.\n",
    )])
    .expect("discover Markdown static routes");

    assert_eq!(
        routes,
        vec![StaticMarkdownRoute {
            path: "/install".to_string(),
            aliases: Vec::new(),
            alias: "Install".to_string(),
            title: Some("Install Terlan".to_string()),
            layout: None,
        }]
    );
}

/// Verifies explicit page titles override Markdown heading defaults.
///
/// Inputs:
/// - A Markdown import with `@page.title` metadata.
/// - Parsed Markdown content with a different heading.
///
/// Output:
/// - Static Markdown route whose title is the explicit metadata value.
///
/// Transformation:
/// - Confirms route discovery prefers user-declared metadata over fallback
///   document headings.
#[test]
fn markdown_static_routes_prefer_explicit_title_over_heading() {
    let routes = discover_markdown_static_routes(&[markdown_input_with_body(
        "Install",
        "content/install.terl.md",
        crate::terlan_html::PageMetadata {
            title: Some("Custom Title".to_string()),
            ..Default::default()
        },
        "# Install Terlan\n",
    )])
    .expect("discover Markdown static routes");

    assert_eq!(routes[0].title.as_deref(), Some("Custom Title"));
}

/// Verifies explicit `@page.route` metadata overrides path inference.
///
/// Inputs:
/// - A Markdown import with route, title, and layout metadata.
///
/// Output:
/// - Test passes when route discovery keeps the explicit route and metadata.
///
/// Transformation:
/// - Uses metadata route directly, while forwarding title/layout for later
///   static rendering.
#[test]
fn markdown_static_routes_use_page_route_override() {
    let routes = discover_markdown_static_routes(&[markdown_input(
        "Install",
        "content/guides/install.terl.md",
        crate::terlan_html::PageMetadata {
            title: Some("Install".to_string()),
            route: Some("/install".to_string()),
            layout: Some("docs".to_string()),
            ..Default::default()
        },
    )])
    .expect("discover Markdown route override");

    assert_eq!(
        routes,
        vec![StaticMarkdownRoute {
            path: "/install".to_string(),
            aliases: Vec::new(),
            alias: "Install".to_string(),
            title: Some("Install".to_string()),
            layout: Some("docs".to_string()),
        }]
    );
}

#[test]
fn markdown_static_routes_preserve_validated_aliases() {
    let routes = discover_markdown_static_routes(&[markdown_input(
        "Install",
        "content/guides/install.terl.md",
        crate::terlan_html::PageMetadata {
            aliases: vec!["/start".to_string(), "/docs/install".to_string()],
            ..Default::default()
        },
    )])
    .expect("discover Markdown aliases");

    assert_eq!(routes[0].aliases, vec!["/start", "/docs/install"]);
}

#[test]
fn markdown_static_routes_reject_alias_collisions() {
    let error = discover_markdown_static_routes(&[
        markdown_input(
            "Install",
            "content/install.terl.md",
            crate::terlan_html::PageMetadata {
                aliases: vec!["/start".to_string()],
                ..Default::default()
            },
        ),
        markdown_input(
            "Start",
            "content/start.terl.md",
            crate::terlan_html::PageMetadata::default(),
        ),
    ])
    .expect_err("alias collision should fail");

    assert!(error.contains("static Markdown alias `/start`"));
}

#[test]
fn markdown_static_routes_reject_normalized_self_aliases() {
    let error = discover_markdown_static_routes(&[markdown_input(
        "Install",
        "content/install.terl.md",
        crate::terlan_html::PageMetadata {
            aliases: vec!["/install/".to_string()],
            ..Default::default()
        },
    )])
    .expect_err("normalized self alias should fail");

    assert!(error.contains("static Markdown alias `/install/`"));
}

#[test]
fn markdown_static_routes_reject_alias_handler_collisions() {
    let markdown_routes = discover_markdown_static_routes(&[markdown_input(
        "Install",
        "content/install.terl.md",
        crate::terlan_html::PageMetadata {
            aliases: vec!["/start".to_string()],
            ..Default::default()
        },
    )])
    .expect("discover Markdown aliases");
    let handler_routes = vec![StaticRoute {
        path: "/start".to_string(),
        handler: "start".to_string(),
    }];

    let error = reject_static_route_path_collisions(&handler_routes, &markdown_routes)
        .expect_err("handler and Markdown alias collision should fail");
    assert!(error.contains("also declared as an alias"));
}

#[test]
fn generated_docs_routes_reject_declared_route_collisions() {
    let handler_routes = vec![StaticRoute {
        path: "/blog/archive".to_string(),
        handler: "archive".to_string(),
    }];
    let error = reject_generated_static_route_path_collisions(
        &handler_routes,
        &[],
        &["/blog/archive".to_string()],
    )
    .expect_err("generated route collision should fail");

    assert!(error.contains("generated documentation route `/blog/archive`"));
}

#[test]
fn markdown_static_routes_reject_unsafe_alias_paths() {
    let error = discover_markdown_static_routes(&[markdown_input(
        "Install",
        "content/install.terl.md",
        crate::terlan_html::PageMetadata {
            aliases: vec!["/docs/../install".to_string()],
            ..Default::default()
        },
    )])
    .expect_err("unsafe alias should fail");

    assert!(error.contains("invalid static Markdown alias"));
}

#[test]
fn markdown_alias_redirect_uses_base_relative_canonical_url() {
    let html = render_static_markdown_alias_html("/docs/install", Some("Install & Run"));

    assert!(html.contains("content=\"0; url=docs/install/\""));
    assert!(html.contains("href=\"docs/install/\""));
    assert!(html.contains("Install&#32;&amp;&#32;Run"));
}

/// Verifies duplicate content routes are rejected.
///
/// Inputs:
/// - Two Markdown imports that resolve to the same static route.
///
/// Output:
/// - Test passes when discovery returns a duplicate-route error.
///
/// Transformation:
/// - Compares route paths after metadata override and path inference.
#[test]
fn markdown_static_routes_reject_duplicate_paths() {
    let error = discover_markdown_static_routes(&[
        markdown_input(
            "Install",
            "content/install.terl.md",
            crate::terlan_html::PageMetadata::default(),
        ),
        markdown_input(
            "AlsoInstall",
            "content/also-install.terl.md",
            crate::terlan_html::PageMetadata {
                route: Some("/install".to_string()),
                ..Default::default()
            },
        ),
    ])
    .expect_err("duplicate route should fail");

    assert!(error.contains("duplicate static Markdown route `/install`"));
}

/// Verifies unsafe Markdown source paths are rejected.
///
/// Inputs:
/// - A Markdown import path with a parent-directory segment.
///
/// Output:
/// - Test passes when discovery refuses to infer a route.
///
/// Transformation:
/// - Rejects unsafe source path segments before output route construction.
#[test]
fn markdown_static_routes_reject_parent_directory_segments() {
    let error = discover_markdown_static_routes(&[markdown_input(
        "Secret",
        "content/../secret.terl.md",
        crate::terlan_html::PageMetadata::default(),
    )])
    .expect_err("parent directory route should fail");

    assert!(error.contains("invalid static Markdown source path"));
}
