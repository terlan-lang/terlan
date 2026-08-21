use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_html::{HtmlAttrValue, HtmlNode};

/// Validates one generated static HTML artifact.
///
/// Inputs:
/// - `html`: rendered HTML text produced by the static-site command.
/// - `target`: generated output path used only for diagnostic context.
///
/// Output:
/// - `Ok(())` when `terlan_html` accepts the generated HTML.
/// - `Err(String)` containing CLI-ready diagnostics when validation fails.
///
/// Transformation:
/// - Delegates HTML checking to `terlan_html` and converts structured
///   diagnostics into newline-separated CLI text.
pub(crate) fn validate_static_html_output(html: &str, target: &Path) -> Result<(), String> {
    crate::terlan_html::validate_html_output(html, target).map_err(format_html_diagnostics)
}

/// Validates generated or copied CSS output files.
///
/// Inputs:
/// - `css_files`: output paths for CSS assets selected by the static command.
///
/// Output:
/// - `Ok(())` when every CSS file can be read and validated.
/// - `Err(String)` when a file cannot be read or CSS validation reports
///   diagnostics.
///
/// Transformation:
/// - Reads each CSS file from disk, delegates CSS parsing/validation to
///   `terlan_html`, and formats any structured diagnostics for CLI output.
pub(crate) fn validate_static_css_output_files(css_files: &[PathBuf]) -> Result<(), String> {
    for path in css_files {
        let source = fs::read_to_string(path).map_err(|err| {
            format!(
                "failed to read static CSS output `{}`: {}",
                path.display(),
                err
            )
        })?;
        crate::terlan_html::validate_css(&source, path).map_err(format_html_diagnostics)?;
    }

    Ok(())
}

#[derive(Debug)]
struct StaticHtmlLinks {
    ids: BTreeSet<String>,
    hrefs: Vec<String>,
}

/// Validates links between emitted static-site files.
///
/// Inputs:
/// - `out_dir`: completed static output directory.
/// - `base_path`: optional HTML base path injected into generated pages.
///
/// Output:
/// - `Ok(())` when every local anchor target exists, including fragments.
/// - A path-specific error for the first broken or escaping link.
///
/// Transformation:
/// - Parses generated HTML with Terlan's output parser, inventories ids and
///   anchors, resolves links with the emitted base-path semantics, and checks
///   the resulting file plus fragment targets without fetching the network.
pub(crate) fn validate_static_internal_links(
    out_dir: &Path,
    base_path: Option<&str>,
) -> Result<(), String> {
    let mut html_paths = Vec::new();
    collect_static_html_paths(out_dir, out_dir, &mut html_paths)?;
    html_paths.sort();

    let mut pages = BTreeMap::new();
    for relative_path in html_paths {
        let target = out_dir.join(&relative_path);
        let html = fs::read_to_string(&target).map_err(|error| {
            format!(
                "failed to read static HTML output `{}`: {error}",
                target.display()
            )
        })?;
        let nodes = crate::terlan_html::parse_html_output_nodes(&html, &target)
            .map_err(format_html_diagnostics)?;
        let mut links = StaticHtmlLinks {
            ids: BTreeSet::new(),
            hrefs: Vec::new(),
        };
        collect_static_html_links(&nodes, &mut links);
        pages.insert(relative_path, links);
    }

    for (source_path, links) in &pages {
        for href in &links.hrefs {
            let Some((target_path, fragment)) = resolve_static_href(source_path, href, base_path)?
            else {
                continue;
            };
            let target = out_dir.join(&target_path);
            if !target.is_file() {
                return Err(format!(
                    "broken static link in `{}`: `{}` resolves to missing `{}`",
                    out_dir.join(source_path).display(),
                    href,
                    target.display()
                ));
            }
            if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty()) {
                let Some(target_page) = pages.get(&target_path) else {
                    return Err(format!(
                        "broken static fragment link in `{}`: `{}` targets a non-HTML file",
                        out_dir.join(source_path).display(),
                        href
                    ));
                };
                if !target_page.ids.contains(&fragment) {
                    return Err(format!(
                        "broken static fragment link in `{}`: `{}` cannot find id `{}` in `{}`",
                        out_dir.join(source_path).display(),
                        href,
                        fragment,
                        target.display()
                    ));
                }
            }
        }
    }

    Ok(())
}

fn collect_static_html_paths(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "failed to inspect static output directory `{}`: {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "failed to inspect static output directory `{}`: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "failed to inspect static output `{}`: {error}",
                path.display()
            )
        })?;
        if file_type.is_dir() {
            collect_static_html_paths(root, &path, paths)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "html") {
            let relative = path.strip_prefix(root).map_err(|error| {
                format!(
                    "failed to relativize static output `{}`: {error}",
                    path.display()
                )
            })?;
            paths.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn collect_static_html_links(nodes: &[HtmlNode], links: &mut StaticHtmlLinks) {
    for node in nodes {
        let HtmlNode::Element(element) = node else {
            continue;
        };
        let generated_hidden_anchor = element.name == "a"
            && element.attrs.iter().any(|attr| {
                attr.name == "aria-hidden"
                    && matches!(&attr.value, Some(HtmlAttrValue::Text(value)) if value == "true")
            })
            && element.attrs.iter().any(|attr| {
                attr.name == "class"
                    && matches!(&attr.value, Some(HtmlAttrValue::Text(value)) if value.split_whitespace().any(|class| class == "anchor"))
            });
        for attr in &element.attrs {
            let Some(HtmlAttrValue::Text(value)) = &attr.value else {
                continue;
            };
            if attr.name == "id" {
                links.ids.insert(value.clone());
            } else if element.name == "a" && attr.name == "href" && !generated_hidden_anchor {
                links.hrefs.push(value.clone());
            }
        }
        collect_static_html_links(&element.children, links);
    }
}

fn resolve_static_href(
    source_path: &Path,
    href: &str,
    base_path: Option<&str>,
) -> Result<Option<(PathBuf, Option<String>)>, String> {
    if href.starts_with("//") || static_href_has_scheme(href) {
        return Ok(None);
    }
    if base_path.is_some() && href.starts_with('#') {
        return Err(format!(
            "static fragment-only link `{href}` is unsafe with an HTML base path; include the page route before the fragment"
        ));
    }

    let (without_fragment, fragment) = href
        .split_once('#')
        .map_or((href, None), |(path, fragment)| (path, Some(fragment)));
    let path = without_fragment.split('?').next().unwrap_or_default();
    let decoded_fragment = fragment.map(|value| {
        percent_encoding::percent_decode_str(value)
            .decode_utf8_lossy()
            .into_owned()
    });

    let (relative_url, base_rooted) = match base_path {
        Some(base) if path.starts_with('/') => {
            let Some(relative) = path.strip_prefix(base) else {
                return Err(format!(
                    "static link `{href}` points outside configured base path `{base}`"
                ));
            };
            (relative, true)
        }
        Some(_) => (path, true),
        None if path.starts_with('/') => (path.trim_start_matches('/'), true),
        None => (path, false),
    };

    let mut segments = if base_rooted {
        Vec::new()
    } else {
        source_path
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    if relative_url.is_empty() && !base_rooted {
        return Ok(Some((source_path.to_path_buf(), decoded_fragment)));
    }
    let trailing_slash = relative_url.is_empty() || relative_url.ends_with('/');
    for raw_segment in relative_url.split('/') {
        let segment = percent_encoding::percent_decode_str(raw_segment)
            .decode_utf8_lossy()
            .into_owned();
        if segment.contains(['/', '\\', '\0']) {
            return Err(format!(
                "static link `{href}` contains an encoded path separator"
            ));
        }
        match segment.as_str() {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(format!("static link `{href}` escapes the output directory"));
                }
            }
            _ => segments.push(segment),
        }
    }

    let mut target = segments.iter().collect::<PathBuf>();
    if trailing_slash || target.as_os_str().is_empty() || target.extension().is_none() {
        target.push("index.html");
    }
    Ok(Some((target, decoded_fragment)))
}

fn static_href_has_scheme(href: &str) -> bool {
    let Some(colon) = href.find(':') else {
        return false;
    };
    let boundary = href.find(['/', '?', '#']).unwrap_or(href.len());
    if colon >= boundary {
        return false;
    }
    let mut scheme = href[..colon].chars();
    scheme.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && scheme.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

/// Formats HTML/CSS diagnostics for CLI display.
///
/// Inputs:
/// - `diagnostics`: structured diagnostics returned by `terlan_html`.
///
/// Output:
/// - A newline-separated message string suitable for `stderr`.
///
/// Transformation:
/// - Preserves diagnostic paths when present and falls back to the diagnostic
///   message when a path is unavailable.
fn format_html_diagnostics(diagnostics: Vec<crate::terlan_html::HtmlDiagnostic>) -> String {
    diagnostics
        .into_iter()
        .map(|diagnostic| match diagnostic.path {
            Some(path) => format!("{}: {}", path.display(), diagnostic.message),
            None => diagnostic.message,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "mod_test.rs"]
#[cfg(test)]
mod mod_test;
