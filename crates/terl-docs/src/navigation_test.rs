use super::*;
use crate::{ContentBuildPolicy, ContentKind};

fn production() -> ContentBuildPolicy {
    ContentBuildPolicy::production("2026-08-16").expect("valid production cutoff")
}

fn page(title: &str, route: &str, parent: Option<&str>, weight: i32) -> ContentPage {
    ContentPage {
        title: title.to_string(),
        navigation_title: None,
        route: route.to_string(),
        description: None,
        section: None,
        kind: ContentKind::Docs,
        published_at: None,
        summary: None,
        authors: Vec::new(),
        tags: Vec::new(),
        draft: false,
        parent: parent.map(str::to_string),
        weight: Some(weight),
        body_text: String::new(),
        headings: Vec::new(),
    }
}

#[test]
fn renders_ordered_tree_breadcrumbs_and_pagination() {
    let navigation = build_navigation(
        &[
            page("Home", "/", None, 0),
            page("Blog", "/blog", None, 20),
            page("Docs", "/docs", None, 10),
            page("Language", "/docs/language", Some("/docs"), 20),
            page(
                "Getting started",
                "/docs/getting-started",
                Some("/docs"),
                10,
            ),
        ],
        &production(),
    )
    .expect("build navigation");
    let page = navigation
        .render_page("/docs/getting-started")
        .expect("render page navigation");

    assert!(
        page.navigation_html.find("Docs").unwrap() < page.navigation_html.find("Blog").unwrap()
    );
    assert!(page.navigation_html.contains("class=\"is-current\""));
    assert!(page
        .navigation_html
        .contains("data-slot=\"sidebar-menu-button\""));
    assert!(page
        .navigation_html
        .contains("data-slot=\"sidebar-menu-sub-button\""));
    assert!(page.navigation_html.contains("data-active=\"true\""));
    assert!(page
        .breadcrumbs_html
        .contains("data-slot=\"breadcrumb-separator\" aria-hidden=\"true\">/</li>"));
    assert!(page
        .breadcrumbs_html
        .contains("data-slot=\"breadcrumb-page\" aria-current=\"page\""));
    assert!(page.pagination_html.contains("Language"));
    assert!(page
        .pagination_html
        .contains("data-slot=\"pagination-next\""));
    assert!(page.toc_html.is_empty());
    let encoded = navigation.to_json().expect("encode navigation");
    let value: serde_json::Value = serde_json::from_str(&encoded).expect("decode navigation");
    assert_eq!(value["items"][2]["url"], "docs/getting-started/");
    assert_eq!(value["items"][2]["depth"], 1);
}

#[test]
fn renders_page_table_of_contents_from_document_headings() {
    let mut docs = page("Docs", "/docs", None, 10);
    docs.headings = vec![
        crate::ContentHeading {
            level: 1,
            title: "Docs".to_string(),
            id: "docs".to_string(),
        },
        crate::ContentHeading {
            level: 2,
            title: "Install & run".to_string(),
            id: "install-run".to_string(),
        },
        crate::ContentHeading {
            level: 3,
            title: "Configure".to_string(),
            id: "configure".to_string(),
        },
    ];
    let navigation = build_navigation(&[docs], &production()).expect("build navigation");
    let rendered = navigation.render_page("/docs").expect("render docs page");

    assert!(rendered.toc_html.contains("aria-label=\"On this page\""));
    assert!(rendered
        .toc_html
        .contains("href=\"docs/#install-run\">Install &amp; run</a>"));
    assert!(rendered.toc_html.contains("data-level=\"3\""));
    assert!(!rendered.toc_html.contains("href=\"docs/#docs\""));
}

#[test]
fn rejects_missing_parents_cycles_and_duplicate_weights() {
    let missing = build_navigation(
        &[page("Child", "/child", Some("/missing"), 0)],
        &production(),
    )
    .expect_err("missing parent should fail");
    assert!(missing.contains("missing parent"));

    let cycle = build_navigation(
        &[
            page("One", "/one", Some("/two"), 0),
            page("Two", "/two", Some("/one"), 0),
        ],
        &production(),
    )
    .expect_err("cycle should fail");
    assert!(cycle.contains("cycle"));

    let duplicate = build_navigation(
        &[page("One", "/one", None, 10), page("Two", "/two", None, 10)],
        &production(),
    )
    .expect_err("duplicate sibling weights should fail");
    assert!(duplicate.contains("share weight 10"));
}
