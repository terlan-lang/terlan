use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{relative_url, ContentBuildPolicy, ContentKind, ContentPage, ARTIFACT_SCHEMA_VERSION};

/// Number of posts rendered on one generated collection page.
pub const BLOG_PAGE_SIZE: usize = 10;

/// A generated blog collection body ready for the site's typed layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlogCollectionPage {
    pub route: String,
    pub title: String,
    pub children_html: String,
}

/// Deterministic archive, tag, and author pages for a visible blog catalog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlogCollections {
    pub pages: Vec<BlogCollectionPage>,
}

impl BlogCollections {
    /// Encodes collection routes and pagination metadata for tooling and stale cleanup.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let pages = self
            .pages
            .iter()
            .map(|page| CollectionIndexPage {
                title: page.title.clone(),
                url: relative_url(&page.route),
            })
            .collect();
        serde_json::to_string_pretty(&CollectionIndex {
            version: ARTIFACT_SCHEMA_VERSION,
            pages,
        })
    }
}

/// Builds paginated archive, tag, and author collections.
pub fn build_blog_collections(
    pages: &[ContentPage],
    policy: &ContentBuildPolicy,
) -> Result<BlogCollections, String> {
    let posts = visible_posts(pages, policy)?;
    if posts.is_empty() {
        return Ok(BlogCollections::default());
    }

    let mut generated = Vec::new();
    append_collection_pages(
        &mut generated,
        "/blog/archive",
        "Blog archive",
        "archive",
        None,
        &posts,
    );

    let tags = group_terms(&posts, |post| &post.tags, "tag")?;
    for (slug, group) in tags {
        append_collection_pages(
            &mut generated,
            &format!("/blog/tags/{slug}"),
            &format!("Posts tagged “{}”", group.label),
            "tag",
            Some(&group.label),
            &group.posts,
        );
    }

    let authors = group_terms(&posts, |post| &post.authors, "author")?;
    for (slug, group) in authors {
        append_collection_pages(
            &mut generated,
            &format!("/blog/authors/{slug}"),
            &format!("Posts by {}", group.label),
            "author",
            Some(&group.label),
            &group.posts,
        );
    }

    generated.sort_by(|left, right| left.route.cmp(&right.route));
    Ok(BlogCollections { pages: generated })
}

#[derive(Debug, Clone)]
struct CollectionPost {
    title: String,
    route: String,
    published_at: String,
    summary: Option<String>,
    description: Option<String>,
    authors: Vec<String>,
    tags: Vec<String>,
    draft: bool,
}

#[derive(Debug)]
struct TermGroup {
    label: String,
    posts: Vec<CollectionPost>,
}

fn visible_posts(
    pages: &[ContentPage],
    policy: &ContentBuildPolicy,
) -> Result<Vec<CollectionPost>, String> {
    let mut posts = Vec::new();
    for page in pages
        .iter()
        .filter(|page| page.is_visible(policy) && page.kind == ContentKind::Blog)
    {
        let published_at = page
            .published_at
            .as_deref()
            .ok_or_else(|| format!("blog page `{}` requires @page.date", page.route))?;
        if !crate::is_iso_date(published_at) {
            return Err(format!(
                "blog page `{}` has invalid @page.date `{published_at}`; expected YYYY-MM-DD",
                page.route
            ));
        }
        posts.push(CollectionPost {
            title: page.title.clone(),
            route: page.route.clone(),
            published_at: published_at.to_string(),
            summary: page.summary.clone(),
            description: page.description.clone(),
            authors: deduplicate_terms(&page.authors),
            tags: deduplicate_terms(&page.tags),
            draft: page.draft,
        });
    }
    posts.sort_by(|left, right| {
        right
            .published_at
            .cmp(&left.published_at)
            .then_with(|| left.route.cmp(&right.route))
    });
    Ok(posts)
}

fn deduplicate_terms(terms: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    terms
        .iter()
        .filter_map(|term| {
            let key = term.to_lowercase();
            seen.insert(key).then(|| term.clone())
        })
        .collect()
}

fn group_terms(
    posts: &[CollectionPost],
    terms: impl Fn(&CollectionPost) -> &[String],
    term_kind: &str,
) -> Result<BTreeMap<String, TermGroup>, String> {
    let mut groups = BTreeMap::<String, TermGroup>::new();
    for post in posts {
        for label in terms(post) {
            let slug = slugify(label).ok_or_else(|| {
                format!("blog {term_kind} `{label}` cannot produce a URL-safe slug")
            })?;
            let group = groups.entry(slug.clone()).or_insert_with(|| TermGroup {
                label: label.clone(),
                posts: Vec::new(),
            });
            if group.label != *label {
                return Err(format!(
                    "blog {term_kind}s `{}` and `{label}` produce the same slug `{slug}`",
                    group.label
                ));
            }
            group.posts.push(post.clone());
        }
    }
    Ok(groups)
}

fn append_collection_pages(
    output: &mut Vec<BlogCollectionPage>,
    base_route: &str,
    base_title: &str,
    collection_kind: &str,
    term: Option<&str>,
    posts: &[CollectionPost],
) {
    let page_count = posts.len().div_ceil(BLOG_PAGE_SIZE);
    for (index, page_posts) in posts.chunks(BLOG_PAGE_SIZE).enumerate() {
        let page_number = index + 1;
        let route = collection_route(base_route, page_number);
        let title = if page_number == 1 {
            base_title.to_string()
        } else {
            format!("{base_title} — Page {page_number}")
        };
        let children_html = render_collection_html(
            collection_kind,
            term,
            page_posts,
            base_route,
            page_number,
            page_count,
        );
        output.push(BlogCollectionPage {
            route,
            title,
            children_html,
        });
    }
}

fn collection_route(base_route: &str, page_number: usize) -> String {
    if page_number == 1 {
        base_route.to_string()
    } else {
        format!("{base_route}/page/{page_number}")
    }
}

fn render_collection_html(
    collection_kind: &str,
    term: Option<&str>,
    posts: &[CollectionPost],
    base_route: &str,
    page_number: usize,
    page_count: usize,
) -> String {
    let mut out = format!(
        "<section class=\"blog-collection\" data-terl-docs-collection=\"{}\"",
        escape_html_attr(collection_kind)
    );
    if let Some(term) = term {
        out.push_str(" data-terl-docs-term=\"");
        out.push_str(&escape_html_attr(term));
        out.push('"');
    }
    out.push_str("><ol class=\"blog-post-list\">");
    for post in posts {
        render_post(&mut out, post);
    }
    out.push_str("</ol>");
    render_collection_pagination(&mut out, base_route, page_number, page_count);
    out.push_str("</section>");
    out
}

fn render_post(out: &mut String, post: &CollectionPost) {
    out.push_str("<li><article class=\"blog-post-card\"><h2><a href=\"");
    out.push_str(&escape_html_attr(&relative_url(&post.route)));
    out.push_str("\">");
    out.push_str(&escape_html_text(&post.title));
    out.push_str("</a></h2><p class=\"blog-post-meta\"><time datetime=\"");
    out.push_str(&escape_html_attr(&post.published_at));
    out.push_str("\">");
    out.push_str(&escape_html_text(&post.published_at));
    out.push_str("</time>");
    if !post.authors.is_empty() {
        out.push_str("<span aria-hidden=\"true\"> · </span><span>By ");
        for (index, author) in post.authors.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str("<a href=\"blog/authors/");
            out.push_str(&slugify(author).expect("validated author slug"));
            out.push_str("/\">");
            out.push_str(&escape_html_text(author));
            out.push_str("</a>");
        }
        out.push_str("</span>");
    }
    if post.draft {
        out.push_str(" <span class=\"blog-post-draft\">Draft</span>");
    }
    out.push_str("</p>");
    if let Some(summary) = post.summary.as_deref().or(post.description.as_deref()) {
        out.push_str("<p>");
        out.push_str(&escape_html_text(summary));
        out.push_str("</p>");
    }
    if !post.tags.is_empty() {
        out.push_str("<ul class=\"blog-tag-list\" aria-label=\"Tags\">");
        for tag in &post.tags {
            out.push_str("<li><a href=\"blog/tags/");
            out.push_str(&slugify(tag).expect("validated tag slug"));
            out.push_str("/\">");
            out.push_str(&escape_html_text(tag));
            out.push_str("</a></li>");
        }
        out.push_str("</ul>");
    }
    out.push_str("</article></li>");
}

fn render_collection_pagination(
    out: &mut String,
    base_route: &str,
    page_number: usize,
    page_count: usize,
) {
    if page_count <= 1 {
        return;
    }
    out.push_str("<nav class=\"blog-pagination\" aria-label=\"Blog collection pages\"><p>Page ");
    out.push_str(&page_number.to_string());
    out.push_str(" of ");
    out.push_str(&page_count.to_string());
    out.push_str("</p><ul>");
    if page_number > 1 {
        out.push_str("<li><a rel=\"prev\" href=\"");
        out.push_str(&escape_html_attr(&relative_url(&collection_route(
            base_route,
            page_number - 1,
        ))));
        out.push_str("\">Newer posts</a></li>");
    }
    if page_number < page_count {
        out.push_str("<li><a rel=\"next\" href=\"");
        out.push_str(&escape_html_attr(&relative_url(&collection_route(
            base_route,
            page_number + 1,
        ))));
        out.push_str("\">Older posts</a></li>");
    }
    out.push_str("</ul></nav>");
}

fn slugify(value: &str) -> Option<String> {
    let mut slug = String::new();
    let mut separator = false;
    for byte in value.bytes() {
        let character = byte as char;
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    (!slug.is_empty()).then_some(slug)
}

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attr(value: &str) -> String {
    escape_html_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[derive(Debug, Serialize)]
struct CollectionIndex {
    version: u32,
    pages: Vec<CollectionIndexPage>,
}

#[derive(Debug, Serialize)]
struct CollectionIndexPage {
    title: String,
    url: String,
}
