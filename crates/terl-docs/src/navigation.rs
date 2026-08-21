use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{ContentBuildPolicy, ContentPage, ARTIFACT_SCHEMA_VERSION};

/// Validated, deterministically ordered documentation navigation graph.
#[derive(Debug, Clone)]
pub struct Navigation {
    pages: BTreeMap<String, NavigationPage>,
    children: BTreeMap<Option<String>, Vec<String>>,
    ordered_routes: Vec<String>,
}

/// Trusted layout fragments generated for one current page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageNavigation {
    pub navigation_html: String,
    pub breadcrumbs_html: String,
    pub pagination_html: String,
    pub toc_html: String,
}

#[derive(Debug, Clone)]
struct NavigationPage {
    title: String,
    route: String,
    parent: Option<String>,
    weight: Option<i32>,
    depth: usize,
    headings: Vec<crate::ContentHeading>,
}

/// Builds and validates the navigation graph for a complete content set.
pub fn build_navigation(
    pages: &[ContentPage],
    policy: &ContentBuildPolicy,
) -> Result<Navigation, String> {
    let mut by_route = BTreeMap::new();
    for page in pages.iter().filter(|page| page.is_visible(policy)) {
        if by_route.contains_key(&page.route) {
            return Err(format!(
                "duplicate documentation navigation route `{}`",
                page.route
            ));
        }
        by_route.insert(
            page.route.clone(),
            NavigationPage {
                title: page
                    .navigation_title
                    .clone()
                    .unwrap_or_else(|| page.title.clone()),
                route: page.route.clone(),
                parent: page.parent.clone(),
                weight: page.weight,
                depth: 0,
                headings: page.headings.clone(),
            },
        );
    }

    for page in by_route.values() {
        if page.parent.as_deref() == Some(page.route.as_str()) {
            return Err(format!(
                "documentation page `{}` cannot be its own parent",
                page.route
            ));
        }
        if let Some(parent) = &page.parent {
            if !by_route.contains_key(parent) {
                return Err(format!(
                    "documentation page `{}` references missing parent `{parent}`",
                    page.route
                ));
            }
        }
        validate_parent_chain(page, &by_route)?;
    }

    let mut children = BTreeMap::<Option<String>, Vec<String>>::new();
    for page in by_route.values() {
        children
            .entry(page.parent.clone())
            .or_default()
            .push(page.route.clone());
    }
    for (parent, routes) in &mut children {
        reject_duplicate_sibling_weights(parent.as_deref(), routes, &by_route)?;
        routes.sort_by(|left, right| compare_pages(&by_route[left], &by_route[right]));
    }

    let mut ordered_routes = Vec::with_capacity(by_route.len());
    let root_routes = children.get(&None).cloned().unwrap_or_default();
    for route in root_routes {
        collect_ordered_routes(&route, 0, &children, &mut by_route, &mut ordered_routes);
    }
    if ordered_routes.len() != by_route.len() {
        return Err("documentation navigation contains an unreachable cycle".to_string());
    }

    Ok(Navigation {
        pages: by_route,
        children,
        ordered_routes,
    })
}

impl Navigation {
    /// Renders navigation, breadcrumbs, and previous/next links for one route.
    pub fn render_page(&self, current_route: &str) -> Result<PageNavigation, String> {
        if !self.pages.contains_key(current_route) {
            return Err(format!(
                "documentation navigation has no route `{current_route}`"
            ));
        }
        Ok(PageNavigation {
            navigation_html: self.render_navigation(current_route),
            breadcrumbs_html: self.render_breadcrumbs(current_route),
            pagination_html: self.render_pagination(current_route),
            toc_html: self.render_table_of_contents(current_route),
        })
    }

    /// Encodes a flat, ordered navigation artifact for tooling and clients.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let items = self
            .ordered_routes
            .iter()
            .map(|route| {
                let page = &self.pages[route];
                NavigationItem {
                    title: page.title.clone(),
                    url: relative_url(&page.route),
                    parent: page.parent.as_deref().map(relative_url),
                    depth: page.depth,
                }
            })
            .collect();
        serde_json::to_string_pretty(&NavigationIndex {
            version: ARTIFACT_SCHEMA_VERSION,
            items,
        })
    }

    fn render_navigation(&self, current_route: &str) -> String {
        let ancestors = self.ancestor_routes(current_route);
        let mut out = String::from(
            "<nav class=\"docs-navigation\" data-slot=\"sidebar-content\" aria-label=\"Documentation navigation\"><ul data-slot=\"sidebar-menu\">",
        );
        if let Some(roots) = self.children.get(&None) {
            for route in roots {
                self.render_navigation_item(route, current_route, &ancestors, 0, &mut out);
            }
        }
        out.push_str("</ul></nav>");
        out
    }

    fn render_navigation_item(
        &self,
        route: &str,
        current_route: &str,
        ancestors: &BTreeSet<String>,
        depth: usize,
        out: &mut String,
    ) {
        let page = &self.pages[route];
        let class = if route == current_route {
            " class=\"is-current\""
        } else if ancestors.contains(route) {
            " class=\"is-ancestor\""
        } else {
            ""
        };
        out.push_str("<li data-slot=\"sidebar-menu-item\"");
        out.push_str(class);
        out.push_str("><a data-slot=\"");
        out.push_str(if depth == 0 {
            "sidebar-menu-button"
        } else {
            "sidebar-menu-sub-button"
        });
        out.push_str("\" href=\"");
        out.push_str(&escape_html_attr(&relative_url(route)));
        out.push('"');
        if route == current_route {
            out.push_str(" data-active=\"true\" aria-current=\"page\"");
        }
        out.push('>');
        out.push_str(&escape_html_text(&page.title));
        out.push_str("</a>");
        if let Some(children) = self.children.get(&Some(route.to_string())) {
            out.push_str("<ul data-slot=\"sidebar-menu-sub\">");
            for child in children {
                self.render_navigation_item(child, current_route, ancestors, depth + 1, out);
            }
            out.push_str("</ul>");
        }
        out.push_str("</li>");
    }

    fn render_breadcrumbs(&self, current_route: &str) -> String {
        let mut routes = Vec::new();
        let mut cursor = Some(current_route);
        while let Some(route) = cursor {
            routes.push(route.to_string());
            cursor = self.pages[route].parent.as_deref();
        }
        routes.reverse();
        if self.pages.contains_key("/") && routes.first().is_none_or(|route| route != "/") {
            routes.insert(0, "/".to_string());
        }

        let mut out = String::from(
            "<nav class=\"breadcrumbs\" data-slot=\"breadcrumb\" aria-label=\"Breadcrumb\"><ol data-slot=\"breadcrumb-list\">",
        );
        for (index, route) in routes.iter().enumerate() {
            let page = &self.pages[route];
            if index > 0 {
                out.push_str("<li data-slot=\"breadcrumb-separator\" aria-hidden=\"true\">/</li>");
            }
            out.push_str("<li data-slot=\"breadcrumb-item\">");
            if index + 1 == routes.len() {
                out.push_str("<span data-slot=\"breadcrumb-page\" aria-current=\"page\">");
                out.push_str(&escape_html_text(&page.title));
                out.push_str("</span>");
            } else {
                out.push_str("<a data-slot=\"breadcrumb-link\" href=\"");
                out.push_str(&escape_html_attr(&relative_url(route)));
                out.push_str("\">");
                out.push_str(&escape_html_text(&page.title));
                out.push_str("</a>");
            }
            out.push_str("</li>");
        }
        out.push_str("</ol></nav>");
        out
    }

    fn render_pagination(&self, current_route: &str) -> String {
        let index = self
            .ordered_routes
            .iter()
            .position(|route| route == current_route)
            .expect("current route was validated");
        let previous = index
            .checked_sub(1)
            .map(|index| &self.ordered_routes[index]);
        let next = self.ordered_routes.get(index + 1);
        if previous.is_none() && next.is_none() {
            return String::new();
        }

        let mut out = String::from(
            "<nav class=\"page-pagination\" data-slot=\"pagination\" aria-label=\"Page navigation\"><ul data-slot=\"pagination-content\">",
        );
        if let Some(route) = previous {
            render_pagination_link(&mut out, "prev", "Previous", &self.pages[route]);
        }
        if let Some(route) = next {
            render_pagination_link(&mut out, "next", "Next", &self.pages[route]);
        }
        out.push_str("</ul></nav>");
        out
    }

    fn render_table_of_contents(&self, current_route: &str) -> String {
        let headings = self.pages[current_route]
            .headings
            .iter()
            .filter(|heading| (2..=4).contains(&heading.level) && !heading.title.is_empty())
            .collect::<Vec<_>>();
        if headings.is_empty() {
            return String::new();
        }

        let mut out = String::from(
            "<nav class=\"docs-toc\" data-slot=\"toc\" aria-label=\"On this page\"><p data-slot=\"toc-title\">On this page</p><ol data-slot=\"toc-list\">",
        );
        for heading in headings {
            out.push_str("<li data-slot=\"toc-item\" data-level=\"");
            out.push_str(&heading.level.to_string());
            out.push_str("\"><a data-slot=\"toc-link\" href=\"");
            out.push_str(&escape_html_attr(&relative_url(current_route)));
            out.push('#');
            out.push_str(&escape_html_attr(&heading.id));
            out.push_str("\">");
            out.push_str(&escape_html_text(&heading.title));
            out.push_str("</a></li>");
        }
        out.push_str("</ol></nav>");
        out
    }

    fn ancestor_routes(&self, route: &str) -> BTreeSet<String> {
        let mut ancestors = BTreeSet::new();
        let mut cursor = self.pages[route].parent.as_deref();
        while let Some(parent) = cursor {
            ancestors.insert(parent.to_string());
            cursor = self.pages[parent].parent.as_deref();
        }
        ancestors
    }
}

fn validate_parent_chain(
    page: &NavigationPage,
    pages: &BTreeMap<String, NavigationPage>,
) -> Result<(), String> {
    let mut seen = BTreeSet::from([page.route.as_str()]);
    let mut cursor = page.parent.as_deref();
    while let Some(route) = cursor {
        if !seen.insert(route) {
            return Err(format!(
                "documentation navigation cycle includes `{}` and `{route}`",
                page.route
            ));
        }
        cursor = pages.get(route).and_then(|parent| parent.parent.as_deref());
    }
    Ok(())
}

fn reject_duplicate_sibling_weights(
    parent: Option<&str>,
    routes: &[String],
    pages: &BTreeMap<String, NavigationPage>,
) -> Result<(), String> {
    let mut weights = BTreeMap::new();
    for route in routes {
        let Some(weight) = pages[route].weight else {
            continue;
        };
        if let Some(existing) = weights.insert(weight, route) {
            let parent = parent.unwrap_or("<root>");
            return Err(format!(
                "documentation navigation siblings `{existing}` and `{route}` under `{parent}` share weight {weight}"
            ));
        }
    }
    Ok(())
}

fn compare_pages(left: &NavigationPage, right: &NavigationPage) -> std::cmp::Ordering {
    left.weight
        .is_none()
        .cmp(&right.weight.is_none())
        .then_with(|| left.weight.cmp(&right.weight))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.route.cmp(&right.route))
}

fn collect_ordered_routes(
    route: &str,
    depth: usize,
    children: &BTreeMap<Option<String>, Vec<String>>,
    pages: &mut BTreeMap<String, NavigationPage>,
    ordered: &mut Vec<String>,
) {
    pages.get_mut(route).expect("route exists").depth = depth;
    ordered.push(route.to_string());
    if let Some(nested) = children.get(&Some(route.to_string())) {
        for child in nested {
            collect_ordered_routes(child, depth + 1, children, pages, ordered);
        }
    }
}

fn render_pagination_link(out: &mut String, rel: &str, label: &str, page: &NavigationPage) {
    out.push_str("<li data-slot=\"pagination-item\"><a data-slot=\"pagination-");
    out.push_str(if rel == "prev" { "previous" } else { "next" });
    out.push_str("\" rel=\"");
    out.push_str(rel);
    out.push_str("\" href=\"");
    out.push_str(&escape_html_attr(&relative_url(&page.route)));
    out.push_str("\"><span>");
    out.push_str(label);
    out.push_str("</span>");
    out.push_str(&escape_html_text(&page.title));
    out.push_str("</a></li>");
}

fn relative_url(route: &str) -> String {
    let route = route.trim_matches('/');
    if route.is_empty() {
        "./".to_string()
    } else {
        format!("{route}/")
    }
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
struct NavigationIndex {
    version: u32,
    items: Vec<NavigationItem>,
}

#[derive(Debug, Serialize)]
struct NavigationItem {
    title: String,
    url: String,
    parent: Option<String>,
    depth: usize,
}

#[cfg(test)]
#[path = "navigation_test.rs"]
mod tests;
