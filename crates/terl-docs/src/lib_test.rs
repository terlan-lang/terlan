use super::*;

fn page(title: &str, route: &str) -> ContentPage {
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
        parent: None,
        weight: None,
        body_text: "  searchable\n  body  ".to_string(),
        headings: Vec::new(),
    }
}

fn production() -> ContentBuildPolicy {
    ContentBuildPolicy::production("2026-08-16").expect("valid production cutoff")
}

#[test]
fn search_index_is_route_sorted_and_base_path_relative() {
    let encoded = build_search_index(
        &[page("Install", "/guides/install"), page("Home", "/")],
        &production(),
    )
    .expect("encode search index");
    let value: serde_json::Value = serde_json::from_str(&encoded).expect("decode search index");

    assert_eq!(value["version"], 1);
    assert_eq!(value["documents"][0]["url"], "./");
    assert_eq!(value["documents"][1]["url"], "guides/install/");
    assert_eq!(value["documents"][1]["content"], "searchable body");
}

#[test]
fn blog_index_is_newest_first() {
    let mut older = page("Older", "/blog/older");
    older.kind = ContentKind::Blog;
    older.published_at = Some("2026-08-01".to_string());
    let mut newer = page("Newer", "/blog/newer");
    newer.kind = ContentKind::Blog;
    newer.published_at = Some("2026-08-15".to_string());
    newer.summary = Some("Release summary".to_string());
    newer.authors = vec!["Terlan team".to_string()];
    newer.tags = vec!["release".to_string(), "compiler".to_string()];

    let encoded = build_blog_index(&[older, newer], &production())
        .expect("build blog index")
        .expect("blog index exists");
    let value: serde_json::Value = serde_json::from_str(&encoded).expect("decode blog index");

    assert_eq!(value["posts"][0]["title"], "Newer");
    assert_eq!(value["posts"][0]["summary"], "Release summary");
    assert_eq!(value["posts"][0]["authors"][0], "Terlan team");
    assert_eq!(value["posts"][0]["tags"][1], "compiler");
    assert_eq!(value["posts"][0]["draft"], false);
    assert_eq!(value["posts"][1]["title"], "Older");
}

#[test]
fn blog_pages_require_valid_dates() {
    let mut post = page("Post", "/blog/post");
    post.kind = ContentKind::Blog;
    post.published_at = Some("15 August".to_string());

    let error = build_blog_index(&[post], &production()).expect_err("invalid date should fail");
    assert!(error.contains("expected YYYY-MM-DD"));

    let mut impossible = page("Post", "/blog/post");
    impossible.kind = ContentKind::Blog;
    impossible.published_at = Some("2026-02-30".to_string());
    assert!(build_blog_index(&[impossible], &production()).is_err());
}

#[test]
fn production_artifacts_exclude_drafts_and_preview_includes_them() {
    let published = page("Published", "/blog/published");
    let mut draft = page("Draft", "/blog/draft");
    draft.kind = ContentKind::Blog;
    draft.published_at = Some("2026-08-16".to_string());
    draft.draft = true;

    let production_index = build_search_index(&[published.clone(), draft.clone()], &production())
        .expect("production search index");
    let production_index: serde_json::Value =
        serde_json::from_str(&production_index).expect("decode production index");
    assert_eq!(production_index["documents"].as_array().unwrap().len(), 1);

    let preview = build_search_index(&[published, draft.clone()], &ContentBuildPolicy::preview())
        .expect("preview search index");
    let preview: serde_json::Value = serde_json::from_str(&preview).expect("decode preview index");
    assert_eq!(preview["documents"].as_array().unwrap().len(), 2);

    assert!(build_blog_index(&[draft.clone()], &production())
        .expect("production blog index")
        .is_none());
    let preview_blog = build_blog_index(&[draft], &ContentBuildPolicy::preview())
        .expect("preview blog index")
        .expect("preview includes draft blog");
    assert!(preview_blog.contains("\"draft\": true"));
}

#[test]
fn production_policy_excludes_scheduled_posts_at_an_explicit_cutoff() {
    let mut published = page("Published", "/blog/published");
    published.kind = ContentKind::Blog;
    published.published_at = Some("2026-08-16".to_string());
    let mut scheduled = page("Scheduled", "/blog/scheduled");
    scheduled.kind = ContentKind::Blog;
    scheduled.published_at = Some("2026-08-17".to_string());

    let production_index = build_blog_index(&[published.clone(), scheduled.clone()], &production())
        .expect("production blog index")
        .expect("published post exists");
    assert!(production_index.contains("Published"));
    assert!(!production_index.contains("Scheduled"));

    let preview_index = build_blog_index(&[published, scheduled], &ContentBuildPolicy::preview())
        .expect("preview blog index")
        .expect("preview posts exist");
    assert!(preview_index.contains("Scheduled"));
}

#[test]
fn production_policy_rejects_invalid_cutoff_dates() {
    let error =
        ContentBuildPolicy::production("2026-02-30").expect_err("impossible cutoff should fail");
    assert!(error.contains("expected YYYY-MM-DD"));
}

#[test]
fn blog_collections_generate_archive_tag_and_author_pages() {
    let mut pages = Vec::new();
    for day in 1..=11 {
        let mut post = page(&format!("Post {day}"), &format!("/blog/post-{day}"));
        post.kind = ContentKind::Blog;
        post.published_at = Some(format!("2026-08-{day:02}"));
        post.summary = Some(format!("Summary {day}"));
        post.authors = vec!["Terlan team".to_string()];
        post.tags = vec!["Compiler design".to_string()];
        pages.push(post);
    }

    let collections = build_blog_collections(&pages, &production()).expect("build collections");

    assert_eq!(
        collections
            .pages
            .iter()
            .filter(|page| page.route.starts_with("/blog/archive"))
            .count(),
        2
    );
    let first = collections
        .pages
        .iter()
        .find(|page| page.route == "/blog/archive")
        .expect("first archive page");
    assert!(first.children_html.contains("Post 11"));
    assert!(!first.children_html.contains(">Post 1</a>"));
    assert!(first.children_html.contains("Page 1 of 2"));
    assert!(first
        .children_html
        .contains("href=\"blog/archive/page/2/\""));
    assert!(collections
        .pages
        .iter()
        .any(|page| page.route == "/blog/tags/compiler-design"));
    assert!(collections
        .pages
        .iter()
        .any(|page| page.route == "/blog/authors/terlan-team"));
    let index: serde_json::Value =
        serde_json::from_str(&collections.to_json().expect("encode collection index"))
            .expect("decode collection index");
    assert_eq!(index["version"], 1);
    assert!(index["pages"]
        .as_array()
        .expect("collection pages")
        .iter()
        .any(|page| page["url"] == "blog/archive/page/2/"));
}

#[test]
fn blog_collections_reject_colliding_term_slugs() {
    let mut first = page("First", "/blog/first");
    first.kind = ContentKind::Blog;
    first.published_at = Some("2026-08-01".to_string());
    first.tags = vec!["C++".to_string()];
    let mut second = page("Second", "/blog/second");
    second.kind = ContentKind::Blog;
    second.published_at = Some("2026-08-02".to_string());
    second.tags = vec!["C#".to_string()];

    let error = build_blog_collections(&[first, second], &production())
        .expect_err("colliding slugs should fail");
    assert!(error.contains("same slug `c`"));
}
