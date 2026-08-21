use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::artifacts::SyntaxMarkdownInput;
use crate::terlan_html::HtmlNode;
use crate::terlan_syntax::SyntaxModuleOutput;
use crate::validation::static_output::validate_static_html_output;

use super::render_markdown::render_syntax_static_html_layout;
use super::{static_route_output_path, StaticMarkdownRoute, StaticSyntaxRenderError};

static DOCS_BUILD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Browser adapter selected from the owning project's JS dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DocsBrowserRuntime {
    /// Portable dependency-free ES module emitted for standalone docs sites.
    Standard,
    /// Angular.ts directive bundled through Terlan's pinned web toolchain.
    ManagedAngularTs,
}

/// Selects the docs browser runtime for one static source file.
///
/// Inputs:
/// - Static entrypoint path, absolute or relative to the current directory.
///
/// Output:
/// - Managed Angular.ts when the nearest `terlan.toml` declares the exact
///   compiler-supported JS dependency; otherwise the standard adapter.
///
/// Transformation:
/// - Walks source ancestors, parses only the owning project manifest, and
///   treats the pinned target dependency as an explicit bundling opt-in.
pub(super) fn resolve_docs_browser_runtime(
    source_path: &Path,
) -> Result<DocsBrowserRuntime, String> {
    let absolute = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("cannot resolve current directory: {error}"))?
            .join(source_path)
    };
    let Some(project_dir) = absolute.parent().and_then(|parent| {
        parent
            .ancestors()
            .find(|candidate| candidate.join("terlan.toml").is_file())
    }) else {
        return Ok(DocsBrowserRuntime::Standard);
    };
    let manifest_path = project_dir.join("terlan.toml");
    let manifest = crate::commands::build::project_manifest::read_project_manifest(&manifest_path)?;
    for dependency in &manifest.dependencies {
        if !matches!(
            dependency.scope,
            crate::commands::build::project_manifest::ProjectDependencyScope::Target(
                crate::commands::build::project_manifest::ProjectTarget::Js
            )
        ) {
            continue;
        }
        let crate::commands::build::project_manifest::ProjectDependencySource::Npm {
            package,
            version,
            ..
        } = &dependency.source
        else {
            continue;
        };
        if package == crate::commands::build::web_toolchain::ANGULAR_TS_PACKAGE {
            if !crate::commands::build::web_toolchain::is_managed_js_dependency(package, version) {
                return Err(format!(
                    "error[web_toolchain_drift]: docs Angular.ts dependency must be `{}`, found `{version}` in {}",
                    crate::commands::build::web_toolchain::ANGULAR_TS_VERSION,
                    manifest_path.display()
                ));
            }
            return Ok(DocsBrowserRuntime::ManagedAngularTs);
        }
    }
    Ok(DocsBrowserRuntime::Standard)
}

/// Fully encoded documentation artifacts, prepared before output is mutated.
#[derive(Debug)]
pub(super) struct DocsArtifacts {
    search_index: String,
    blog_index: Option<String>,
    blog_collections_index: Option<String>,
    blog_collection_pages: Vec<terl_docs::BlogCollectionPage>,
    blog_collection_layout: Option<String>,
    blog_collection_navigation: Option<terl_docs::PageNavigation>,
    navigation_index: String,
    page_navigation: BTreeMap<String, terl_docs::PageNavigation>,
    visible_routes: BTreeSet<String>,
}

/// Builds higher-level documentation artifacts from discovered Markdown pages.
///
/// Inputs:
/// - Validated static Markdown routes and their parsed frontend inputs.
///
/// Output:
/// - Encoded search and optional blog indexes, or a metadata error.
///
/// Transformation:
/// - Adapts compiler-owned Markdown nodes and page metadata to the
///   compiler-neutral `terl-docs` content model.
pub(super) fn build_docs_artifacts(
    routes: &[StaticMarkdownRoute],
    inputs: &[SyntaxMarkdownInput],
    policy: &terl_docs::ContentBuildPolicy,
) -> Result<DocsArtifacts, String> {
    let mut pages = Vec::with_capacity(routes.len());
    for route in routes {
        let input = inputs
            .iter()
            .find(|input| input.alias == route.alias)
            .ok_or_else(|| {
                format!(
                    "static Markdown route `{}` references unknown Markdown import `{}`",
                    route.path, route.alias
                )
            })?;
        pages.push(terl_docs::ContentPage {
            title: route
                .title
                .clone()
                .unwrap_or_else(|| fallback_page_title(&route.path)),
            navigation_title: input.metadata.nav_title.clone(),
            route: route.path.clone(),
            description: input.metadata.description.clone(),
            section: input.metadata.section.clone(),
            kind: terl_docs::ContentKind::parse(input.metadata.kind.as_deref())?,
            published_at: input.metadata.date.clone(),
            summary: input.metadata.summary.clone(),
            authors: input.metadata.authors.clone(),
            tags: input.metadata.tags.clone(),
            draft: input.metadata.draft,
            parent: input.metadata.parent.clone(),
            weight: input.metadata.weight,
            body_text: markdown_document_text(&input.document.nodes),
            headings: input
                .document
                .headings
                .iter()
                .map(|heading| terl_docs::ContentHeading {
                    level: heading.level,
                    title: heading.title.clone(),
                    id: heading.id.clone(),
                })
                .collect(),
        });
    }

    let visible_routes = pages
        .iter()
        .filter(|page| page.is_visible(policy))
        .map(|page| page.route.clone())
        .collect::<BTreeSet<_>>();
    let navigation = terl_docs::build_navigation(&pages, policy)?;
    let navigation_index = navigation
        .to_json()
        .map_err(|error| format!("failed to encode static navigation index: {error}"))?;
    let page_navigation = pages
        .iter()
        .filter(|page| page.is_visible(policy))
        .map(|page| {
            navigation
                .render_page(&page.route)
                .map(|rendered| (page.route.clone(), rendered))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let search_index = terl_docs::build_search_index(&pages, policy)
        .map_err(|error| format!("failed to encode static search index: {error}"))?;
    let blog_index = terl_docs::build_blog_index(&pages, policy)?;
    let mut blog_collections = terl_docs::build_blog_collections(&pages, policy)?;
    let blog_root = pages
        .iter()
        .find(|page| page.route == "/blog" && page.is_visible(policy));
    let blog_collection_layout = if blog_root.is_some() && !blog_collections.pages.is_empty() {
        let layout = routes
            .iter()
            .find(|route| route.path == "/blog")
            .and_then(|route| route.layout.clone())
            .ok_or_else(|| {
                "blog root `/blog` requires a typed @page.layout to render generated collections"
                    .to_string()
            })?;
        Some(layout)
    } else {
        blog_collections.pages.clear();
        None
    };
    let blog_collections_index = (!blog_collections.pages.is_empty())
        .then(|| blog_collections.to_json())
        .transpose()
        .map_err(|error| format!("failed to encode static blog collections: {error}"))?;
    let blog_collection_navigation = blog_collection_layout
        .as_ref()
        .and_then(|_| page_navigation.get("/blog").cloned());
    Ok(DocsArtifacts {
        search_index,
        blog_index,
        blog_collections_index,
        blog_collection_pages: blog_collections.pages,
        blog_collection_layout,
        blog_collection_navigation,
        navigation_index,
        page_navigation,
        visible_routes,
    })
}

impl DocsArtifacts {
    /// Returns trusted navigation fragments for a discovered content route.
    pub(super) fn page_navigation(&self, route: &str) -> Option<&terl_docs::PageNavigation> {
        self.page_navigation.get(route)
    }

    /// Returns whether a content route is emitted in the selected build mode.
    pub(super) fn includes_route(&self, route: &str) -> bool {
        self.visible_routes.contains(route)
    }

    /// Returns generated blog collection pages rendered after Markdown routes.
    pub(super) fn blog_collection_pages(&self) -> &[terl_docs::BlogCollectionPage] {
        &self.blog_collection_pages
    }

    /// Returns the site-owned typed layout selected by the `/blog` root page.
    pub(super) fn blog_collection_layout(&self) -> Option<&str> {
        self.blog_collection_layout.as_deref()
    }

    /// Returns the blog-root navigation shell reused by generated collections.
    pub(super) fn blog_collection_navigation(&self) -> Option<&terl_docs::PageNavigation> {
        self.blog_collection_navigation.as_ref()
    }
}

/// Writes prepared documentation artifacts into static output.
///
/// Inputs:
/// - Static output directory and artifacts prepared before page rendering.
///
/// Output:
/// - Success after writing indexes and the search client, or a path-aware
///   filesystem error.
///
/// Transformation:
/// - Creates the namespaced browser asset directory and removes an obsolete
///   blog index when the current content set contains no posts.
pub(super) fn write_docs_artifacts(
    out_dir: &Path,
    artifacts: &DocsArtifacts,
    browser_runtime: DocsBrowserRuntime,
) -> Result<(), String> {
    write_artifact(
        &out_dir.join("search-index.json"),
        artifacts.search_index.as_bytes(),
    )?;
    write_artifact(
        &out_dir.join("navigation.json"),
        artifacts.navigation_index.as_bytes(),
    )?;
    write_artifact(
        &out_dir.join("assets/terl-docs/search-policy.js"),
        terl_docs::SEARCH_POLICY_JS.as_bytes(),
    )?;
    match browser_runtime {
        DocsBrowserRuntime::Standard => write_artifact(
            &out_dir.join("assets/terl-docs/search.js"),
            terl_docs::SEARCH_CLIENT_JS.as_bytes(),
        )?,
        DocsBrowserRuntime::ManagedAngularTs => write_managed_angular_search(out_dir)?,
    }

    let blog_path = out_dir.join("blog-index.json");
    if let Some(blog_index) = &artifacts.blog_index {
        write_artifact(&blog_path, blog_index.as_bytes())?;
    } else if blog_path.exists() {
        fs::remove_file(&blog_path).map_err(|error| {
            format!(
                "failed to remove stale static docs artifact `{}`: {error}",
                blog_path.display()
            )
        })?;
    }
    let collections_path = out_dir.join("blog-collections.json");
    if let Some(index) = &artifacts.blog_collections_index {
        write_artifact(&collections_path, index.as_bytes())?;
    } else if collections_path.exists() {
        fs::remove_file(&collections_path).map_err(|error| {
            format!(
                "failed to remove stale static docs artifact `{}`: {error}",
                collections_path.display()
            )
        })?;
    }
    Ok(())
}

/// Renders generated blog collections through the site's typed `/blog` layout.
pub(super) fn write_blog_collection_pages(
    out_dir: &Path,
    artifacts: &DocsArtifacts,
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, crate::terlan_html::HtmlTemplate>,
    base_path: Option<&str>,
    validate_output: bool,
) -> Result<(), String> {
    remove_stale_blog_collection_pages(out_dir, artifacts)?;
    let Some(layout) = artifacts.blog_collection_layout() else {
        return Ok(());
    };
    let mut navigation = artifacts
        .blog_collection_navigation()
        .cloned()
        .unwrap_or_default();
    navigation.pagination_html.clear();
    navigation.toc_html.clear();

    for page in artifacts.blog_collection_pages() {
        let mut html = render_syntax_static_html_layout(
            module,
            templates,
            layout,
            Some(&page.title),
            &page.children_html,
            Some(&navigation),
        )
        .map_err(|error| match error {
            StaticSyntaxRenderError::Invalid(message) => message,
        })?;
        if let Some(base_path) = base_path {
            html = crate::terlan_html::qualify_html_fragment_links(
                &html,
                &format!("{}/", page.route.trim_matches('/')),
            );
            html = crate::terlan_html::inject_html_base_path(&html, base_path);
        }
        let target = out_dir.join(static_route_output_path(&page.route)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create generated blog directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        if validate_output {
            validate_static_html_output(&html, &target)?;
        }
        write_artifact(&target, html.as_bytes())?;
    }
    Ok(())
}

fn remove_stale_blog_collection_pages(
    out_dir: &Path,
    artifacts: &DocsArtifacts,
) -> Result<(), String> {
    let index_path = out_dir.join("blog-collections.json");
    let Ok(encoded) = fs::read_to_string(&index_path) else {
        return Ok(());
    };
    let previous: serde_json::Value = serde_json::from_str(&encoded).map_err(|error| {
        format!(
            "failed to decode existing blog collections `{}`: {error}",
            index_path.display()
        )
    })?;
    let current = artifacts
        .blog_collection_pages()
        .iter()
        .map(|page| page.route.as_str())
        .collect::<BTreeSet<_>>();
    for url in previous["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|page| page["url"].as_str())
    {
        let route = format!("/{}", url.trim_matches('/'));
        if current.contains(route.as_str()) {
            continue;
        }
        if !route.starts_with("/blog/archive")
            && !route.starts_with("/blog/tags/")
            && !route.starts_with("/blog/authors/")
        {
            return Err(format!(
                "existing blog collection index contains unmanaged route `{route}`"
            ));
        }
        let target = out_dir.join(static_route_output_path(&route)?);
        if target.is_file() {
            fs::remove_file(&target).map_err(|error| {
                format!(
                    "failed to remove stale generated blog page `{}`: {error}",
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Emits the Angular.ts search directive as one compiler-managed browser file.
fn write_managed_angular_search(out_dir: &Path) -> Result<(), String> {
    let staging = create_docs_build_staging_dir()?;
    let result = (|| {
        let entry = staging.join("search.js");
        write_artifact(&entry, terl_docs::SEARCH_ANGULAR_CLIENT_JS.as_bytes())?;
        write_artifact(
            &staging.join("search-policy.js"),
            terl_docs::SEARCH_POLICY_JS.as_bytes(),
        )?;
        crate::commands::build::web_toolchain::bundle_managed_angular_entry(
            &entry,
            &out_dir.join("assets/terl-docs"),
            "search.js",
        )
    })();
    let cleanup = fs::remove_dir_all(&staging).map_err(|error| {
        format!(
            "failed to remove compiler docs staging directory `{}`: {error}",
            staging.display()
        )
    });
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Creates a collision-resistant compiler-owned temporary staging directory.
fn create_docs_build_staging_dir() -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("cannot timestamp docs browser build: {error}"))?
        .as_nanos();
    let sequence = DOCS_BUILD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging = std::env::temp_dir().join(format!(
        "terlan-docs-browser-{}-{nanos}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&staging).map_err(|error| {
        format!(
            "cannot create compiler docs staging directory `{}`: {error}",
            staging.display()
        )
    })?;
    Ok(staging)
}

fn write_artifact(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create static docs output directory `{}`: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(path, bytes).map_err(|error| {
        format!(
            "failed to write static docs artifact `{}`: {error}",
            path.display()
        )
    })
}

fn fallback_page_title(route: &str) -> String {
    let segment = route
        .trim_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("Home");
    segment
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "docs_artifacts_inline_test.rs"]
mod tests;

fn markdown_document_text(nodes: &[HtmlNode]) -> String {
    let mut text = String::new();
    collect_node_text(nodes, &mut text);
    text
}

fn collect_node_text(nodes: &[HtmlNode], out: &mut String) {
    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                out.push_str(text);
                out.push(' ');
            }
            HtmlNode::Element(element) => collect_node_text(&element.children, out),
            HtmlNode::Comment(_) | HtmlNode::Doctype(_) | HtmlNode::Slot(_) => {}
        }
    }
}

#[cfg(test)]
#[path = "docs_artifacts_test.rs"]
mod docs_artifacts_test;
