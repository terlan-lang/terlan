use std::path::PathBuf;

use super::*;

fn input(alias: &str, source: &str) -> SyntaxMarkdownInput {
    let path = PathBuf::from(format!("content/{alias}.terl.md"));
    let metadata = crate::terlan_html::extract_page_metadata(source, &path)
        .expect("extract fixture page metadata");
    let document =
        crate::terlan_html::parse_markdown(source, &path).expect("parse fixture Markdown");
    SyntaxMarkdownInput {
        alias: alias.to_string(),
        source_path: path.display().to_string(),
        resolved_path: path,
        metadata,
        document,
    }
}

fn production() -> terl_docs::ContentBuildPolicy {
    terl_docs::ContentBuildPolicy::production("2026-08-16").expect("valid production cutoff")
}

#[test]
fn docs_artifacts_include_search_text_and_sorted_blog_posts() {
    let guide = input(
        "Guide",
        "@page { description = \"Learn Terlan\", section = \"Guides\", nav_title = \"Guide\", weight = 10 }\n\n# Install\n\n## Run it\n\nRun the compiler.\n",
    );
    let post = input(
        "Post",
        "@page { kind = \"blog\", date = \"2026-08-15\", summary = \"Runtime release.\", authors = [\"Terlan team\"], tags = [\"release\"], weight = 20 }\n\n# Release news\n\nA faster runtime.\n",
    );
    let routes = vec![
        StaticMarkdownRoute {
            path: "/guides/install".to_string(),
            aliases: Vec::new(),
            alias: "Guide".to_string(),
            title: Some("Install".to_string()),
            layout: None,
        },
        StaticMarkdownRoute {
            path: "/blog/release".to_string(),
            aliases: Vec::new(),
            alias: "Post".to_string(),
            title: Some("Release news".to_string()),
            layout: None,
        },
    ];

    let artifacts =
        build_docs_artifacts(&routes, &[guide, post], &production()).expect("build docs artifacts");
    let search: serde_json::Value =
        serde_json::from_str(&artifacts.search_index).expect("decode search index");
    let blog: serde_json::Value =
        serde_json::from_str(artifacts.blog_index.as_deref().expect("blog index"))
            .expect("decode blog index");
    let navigation: serde_json::Value =
        serde_json::from_str(&artifacts.navigation_index).expect("decode navigation index");

    assert_eq!(search["documents"][1]["description"], "Learn Terlan");
    assert_eq!(
        search["documents"][1]["content"],
        "Install Run it Run the compiler."
    );
    assert_eq!(blog["posts"][0]["url"], "blog/release/");
    assert_eq!(blog["posts"][0]["summary"], "Runtime release.");
    assert_eq!(blog["posts"][0]["authors"][0], "Terlan team");
    assert_eq!(blog["posts"][0]["tags"][0], "release");
    assert_eq!(navigation["items"][0]["title"], "Guide");
    assert_eq!(navigation["items"][0]["url"], "guides/install/");
    assert!(artifacts
        .page_navigation("/guides/install")
        .expect("guide navigation")
        .navigation_html
        .contains("aria-current=\"page\""));
    assert!(artifacts
        .page_navigation("/guides/install")
        .expect("guide navigation")
        .toc_html
        .contains("href=\"guides/install/#run-it\">Run it</a>"));
}

#[test]
fn docs_artifacts_reject_unknown_content_kinds() {
    let page = input(
        "Unknown",
        "@page { kind = \"announcement\" }\n\n# Unknown\n",
    );
    let route = StaticMarkdownRoute {
        path: "/unknown".to_string(),
        aliases: Vec::new(),
        alias: "Unknown".to_string(),
        title: Some("Unknown".to_string()),
        layout: None,
    };

    let error =
        build_docs_artifacts(&[route], &[page], &production()).expect_err("kind should fail");
    assert!(error.contains("expected `docs`, `blog`, or `page`"));
}

#[test]
fn docs_artifacts_hide_drafts_in_production_and_include_them_in_preview() {
    let draft = input(
        "Draft",
        "@page { kind = \"blog\", date = \"2026-08-16\", draft = true }\n\n# Draft post\n",
    );
    let route = StaticMarkdownRoute {
        path: "/blog/draft".to_string(),
        aliases: Vec::new(),
        alias: "Draft".to_string(),
        title: Some("Draft post".to_string()),
        layout: None,
    };

    let production = build_docs_artifacts(
        std::slice::from_ref(&route),
        std::slice::from_ref(&draft),
        &production(),
    )
    .expect("production artifacts");
    assert!(!production.includes_route("/blog/draft"));
    assert!(!production.search_index.contains("Draft post"));
    assert!(!production.navigation_index.contains("Draft post"));
    assert!(production.blog_index.is_none());

    let preview = build_docs_artifacts(
        &[route],
        &[draft],
        &terl_docs::ContentBuildPolicy::preview(),
    )
    .expect("preview artifacts");
    assert!(preview.includes_route("/blog/draft"));
    assert!(preview.search_index.contains("Draft post"));
    assert!(preview.navigation_index.contains("Draft post"));
    assert!(preview
        .blog_index
        .as_deref()
        .is_some_and(|index| index.contains("\"draft\": true")));
}

#[test]
fn docs_artifacts_generate_collections_for_a_typed_blog_root() {
    let root = input(
        "Blog",
        "@page { title = \"Blog\", kind = \"page\", layout = \"DocsLayout\", weight = 10 }\n\n# Blog\n",
    );
    let post = input(
        "Post",
        "@page { kind = \"blog\", date = \"2026-08-15\", parent = \"/blog\", weight = 10, authors = [\"Terlan team\"], tags = [\"compiler\"] }\n\n# Release\n",
    );
    let routes = vec![
        StaticMarkdownRoute {
            path: "/blog".to_string(),
            aliases: Vec::new(),
            alias: "Blog".to_string(),
            title: Some("Blog".to_string()),
            layout: Some("DocsLayout".to_string()),
        },
        StaticMarkdownRoute {
            path: "/blog/release".to_string(),
            aliases: Vec::new(),
            alias: "Post".to_string(),
            title: Some("Release".to_string()),
            layout: Some("DocsLayout".to_string()),
        },
    ];

    let artifacts =
        build_docs_artifacts(&routes, &[root, post], &production()).expect("build collections");

    assert_eq!(artifacts.blog_collection_layout(), Some("DocsLayout"));
    assert!(artifacts
        .blog_collection_pages()
        .iter()
        .any(|page| page.route == "/blog/archive"));
    assert!(artifacts
        .blog_collection_pages()
        .iter()
        .any(|page| page.route == "/blog/tags/compiler"));
    assert!(artifacts
        .blog_collection_pages()
        .iter()
        .any(|page| page.route == "/blog/authors/terlan-team"));
    assert!(artifacts
        .blog_collections_index
        .as_deref()
        .is_some_and(|index| index.contains("blog/archive/")));
}
