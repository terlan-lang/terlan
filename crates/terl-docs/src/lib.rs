//! Deterministic documentation-site artifacts built on Terlan static pages.
//!
//! This crate deliberately does not parse Markdown or render page templates.
//! Those are compiler primitives owned by `terlc static`. Automatic API
//! documentation extraction also belongs to the compiler side of this
//! boundary. `terl-docs` consumes the resulting content model and adds
//! higher-level documentation product policy: navigation, local search, and
//! ordered blog collections.

use serde::Serialize;

mod blog;
mod navigation;
pub use blog::{build_blog_collections, BlogCollectionPage, BlogCollections, BLOG_PAGE_SIZE};
pub use navigation::{build_navigation, Navigation, PageNavigation};

/// Schema version for generated JSON artifacts.
pub const ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Browser adapter for a `[data-terl-docs-search]` widget.
pub const SEARCH_CLIENT_JS: &str = include_str!("search.js");

/// Angular.ts directive source used by the compiler-managed docs bundle.
pub const SEARCH_ANGULAR_CLIENT_JS: &str = include_str!("search-angular.js");

/// JavaScript ranking policy emitted from `terlan/terl_docs/Search.terl`.
pub const SEARCH_POLICY_JS: &str = include_str!("search-policy.js");

/// High-level role of one content page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContentKind {
    /// A regular documentation page included in site search.
    #[default]
    Docs,
    /// A dated post included in site search and the blog catalog.
    Blog,
    /// A general site page included in site search.
    Page,
}

/// Publication policy for generated documentation artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentBuildPolicy {
    /// Excludes drafts and blog posts dated after the explicit cutoff.
    Production { publish_through: String },
    /// Includes drafts and scheduled posts for local review.
    Preview,
}

impl ContentBuildPolicy {
    /// Creates a deterministic production policy from an ISO publication date.
    pub fn production(publish_through: impl Into<String>) -> Result<Self, String> {
        let publish_through = publish_through.into();
        if !is_iso_date(&publish_through) {
            return Err(format!(
                "invalid publication cutoff `{publish_through}`; expected YYYY-MM-DD"
            ));
        }
        Ok(Self::Production { publish_through })
    }

    /// Creates a preview policy that includes unpublished content.
    pub fn preview() -> Self {
        Self::Preview
    }
}

impl ContentKind {
    /// Parses the stable `@page.kind` spelling.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("docs") {
            "docs" => Ok(Self::Docs),
            "blog" => Ok(Self::Blog),
            "page" => Ok(Self::Page),
            other => Err(format!(
                "unsupported @page.kind `{other}`; expected `docs`, `blog`, or `page`"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Docs => "docs",
            Self::Blog => "blog",
            Self::Page => "page",
        }
    }
}

/// Compiler-neutral page data used to build documentation artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPage {
    pub title: String,
    pub navigation_title: Option<String>,
    pub route: String,
    pub description: Option<String>,
    pub section: Option<String>,
    pub kind: ContentKind,
    pub published_at: Option<String>,
    pub summary: Option<String>,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub draft: bool,
    pub parent: Option<String>,
    pub weight: Option<i32>,
    pub body_text: String,
    pub headings: Vec<ContentHeading>,
}

impl ContentPage {
    /// Returns whether this page belongs in artifacts for `policy`.
    pub fn is_visible(&self, policy: &ContentBuildPolicy) -> bool {
        match policy {
            ContentBuildPolicy::Preview => true,
            ContentBuildPolicy::Production { publish_through } => {
                if self.draft {
                    return false;
                }
                if self.kind != ContentKind::Blog {
                    return true;
                }
                self.published_at
                    .as_deref()
                    .filter(|date| is_iso_date(date))
                    .is_none_or(|date| date <= publish_through.as_str())
            }
        }
    }
}

/// Compiler-neutral Markdown heading used for page-local navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHeading {
    pub level: u8,
    pub title: String,
    pub id: String,
}

/// Builds the versioned, deterministic client-side search index.
pub fn build_search_index(
    pages: &[ContentPage],
    policy: &ContentBuildPolicy,
) -> Result<String, serde_json::Error> {
    let mut documents = pages
        .iter()
        .filter(|page| page.is_visible(policy))
        .map(SearchDocument::from)
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| left.url.cmp(&right.url));
    serde_json::to_string_pretty(&SearchIndex {
        version: ARTIFACT_SCHEMA_VERSION,
        documents,
    })
}

/// Builds the versioned blog catalog, newest post first.
///
/// Blog dates use the intentionally small ISO `YYYY-MM-DD` contract. The
/// lexical order is therefore chronological and does not require a timezone.
pub fn build_blog_index(
    pages: &[ContentPage],
    policy: &ContentBuildPolicy,
) -> Result<Option<String>, String> {
    let mut posts = Vec::new();
    for page in pages
        .iter()
        .filter(|page| page.is_visible(policy) && page.kind == ContentKind::Blog)
    {
        let published_at = page
            .published_at
            .as_deref()
            .ok_or_else(|| format!("blog page `{}` requires @page.date", page.route))?;
        if !is_iso_date(published_at) {
            return Err(format!(
                "blog page `{}` has invalid @page.date `{published_at}`; expected YYYY-MM-DD",
                page.route
            ));
        }
        posts.push(BlogPost {
            title: page.title.clone(),
            url: relative_url(&page.route),
            description: page.description.clone(),
            summary: page.summary.clone(),
            authors: page.authors.clone(),
            tags: page.tags.clone(),
            published_at: published_at.to_string(),
            draft: page.draft,
        });
    }
    if posts.is_empty() {
        return Ok(None);
    }
    posts.sort_by(|left, right| {
        right
            .published_at
            .cmp(&left.published_at)
            .then_with(|| left.url.cmp(&right.url))
    });
    serde_json::to_string_pretty(&BlogIndex {
        version: ARTIFACT_SCHEMA_VERSION,
        posts,
    })
    .map(Some)
    .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
struct SearchIndex {
    version: u32,
    documents: Vec<SearchDocument>,
}

#[derive(Debug, Serialize)]
struct SearchDocument {
    title: String,
    url: String,
    description: Option<String>,
    summary: Option<String>,
    section: Option<String>,
    kind: &'static str,
    authors: Vec<String>,
    tags: Vec<String>,
    content: String,
}

impl From<&ContentPage> for SearchDocument {
    fn from(page: &ContentPage) -> Self {
        Self {
            title: page.title.clone(),
            url: relative_url(&page.route),
            description: page.description.clone(),
            summary: page.summary.clone(),
            section: page.section.clone(),
            kind: page.kind.as_str(),
            authors: page.authors.clone(),
            tags: page.tags.clone(),
            content: normalize_text(&format!(
                "{} {} {}",
                page.body_text,
                page.authors.join(" "),
                page.tags.join(" ")
            )),
        }
    }
}

#[derive(Debug, Serialize)]
struct BlogIndex {
    version: u32,
    posts: Vec<BlogPost>,
}

#[derive(Debug, Serialize)]
struct BlogPost {
    title: String,
    url: String,
    description: Option<String>,
    summary: Option<String>,
    authors: Vec<String>,
    tags: Vec<String>,
    published_at: String,
    draft: bool,
}

fn relative_url(route: &str) -> String {
    let route = route.trim_matches('/');
    if route.is_empty() {
        "./".to_string()
    } else {
        format!("{route}/")
    }
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u16>().ok();
    let month = value[5..7].parse::<u8>().ok();
    let day = value[8..10].parse::<u8>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let days_in_month = match month {
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days_in_month).contains(&day)
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
