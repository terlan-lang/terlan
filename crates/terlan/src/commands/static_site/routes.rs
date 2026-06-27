use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::terlan_html::{HtmlNode, MarkdownDocument};
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};

use std::path::Path;

use crate::commands::artifacts::SyntaxMarkdownInput;

/// Static route from a URL path to a zero-argument HTML handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticRoute {
    pub(crate) path: String,
    pub(crate) handler: String,
}

/// Static route discovered from imported Markdown content.
///
/// Inputs:
/// - Markdown frontend input plus optional `@page` metadata.
///
/// Output:
/// - URL path, import alias, title, and layout data needed by static rendering.
///
/// Transformation:
/// - Carries route-discovery results without coupling content routes to
///   function-handler routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticMarkdownRoute {
    pub(crate) path: String,
    pub(crate) alias: String,
    pub(crate) title: Option<String>,
    pub(crate) layout: Option<String>,
}

/// Parsed static route declarations plus their declaration shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedStaticRoutes {
    pub(crate) routes: Vec<StaticRoute>,
    pub(crate) is_block: bool,
}

/// Discovers formal syntax-output static HTML entrypoints.
///
/// Inputs:
/// - `module`: formal syntax output module.
///
/// Output:
/// - Sorted public zero-argument functions returning template HTML.
///
/// Transformation:
/// - Filters syntax declaration payloads by visibility, arity, and return type.
pub(crate) fn discover_syntax_static_entrypoints(module: &SyntaxModuleOutput) -> Vec<String> {
    let mut entrypoints: Vec<String> = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                is_public,
                ..
            } if *is_public
                && params.is_empty()
                && is_static_html_return_type(&return_type.text) =>
            {
                Some(name.clone())
            }
            _ => None,
        })
        .collect();
    entrypoints.sort();
    entrypoints
}

/// Discovers formal syntax-output static routes.
///
/// Inputs:
/// - `module`: formal syntax output module.
///
/// Output:
/// - Static routes or a validation error string.
///
/// Transformation:
/// - Scans syntax-output config declarations, parses singular or block route
///   syntax, enforces one route block, and rejects duplicate paths.
pub(crate) fn discover_syntax_static_routes(
    module: &SyntaxModuleOutput,
) -> Result<Vec<StaticRoute>, String> {
    let mut routes = Vec::new();
    let mut route_block_seen = false;
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Config { name, text, .. } = &declaration.payload else {
            continue;
        };
        if name != "static" {
            continue;
        }
        let parsed = parse_static_routes_text(text)?;
        if parsed.is_block {
            if route_block_seen {
                return Err("static routes must be declared in a single routes block".to_string());
            }
            route_block_seen = true;
        }
        routes.extend(parsed.routes);
    }
    reject_duplicate_static_routes(&routes)?;
    Ok(routes)
}

/// Discovers static Markdown content routes.
///
/// Inputs:
/// - `inputs`: parsed Markdown imports with source paths and `@page` metadata.
///
/// Output:
/// - Content routes, or an error string when a route is invalid or duplicated.
///
/// Transformation:
/// - Uses explicit `@page.route` when present, otherwise infers routes from
///   `content/`-style source paths. Every route is validated through the same
///   output-path rules as explicit static routes.
pub(crate) fn discover_markdown_static_routes(
    inputs: &[SyntaxMarkdownInput],
) -> Result<Vec<StaticMarkdownRoute>, String> {
    let mut routes = Vec::new();
    for input in inputs {
        let path = markdown_static_route_path(input)?;
        static_route_output_path(&path)?;
        routes.push(StaticMarkdownRoute {
            path,
            alias: input.alias.clone(),
            title: input
                .metadata
                .title
                .clone()
                .or_else(|| markdown_document_heading_title(&input.document)),
            layout: input.metadata.layout.clone(),
        });
    }
    reject_duplicate_markdown_static_routes(&routes)?;
    Ok(routes)
}

/// Extracts the first static Markdown heading as a page title.
///
/// Inputs:
/// - `document`: parsed Markdown document with rendered HTML nodes.
///
/// Output:
/// - Heading text from the first `h1` through `h6` element with non-empty
///   static text.
/// - `None` when no static heading text is available.
///
/// Transformation:
/// - Reuses the Markdown-to-HTML parser output instead of reparsing Markdown
///   syntax, then collects text children from the first heading element. Dynamic
///   interpolation slots are ignored for this default so page titles remain
///   deterministic.
fn markdown_document_heading_title(document: &MarkdownDocument) -> Option<String> {
    document
        .nodes
        .iter()
        .find_map(markdown_heading_title_from_node)
}

/// Extracts title text from one parsed Markdown HTML node.
///
/// Inputs:
/// - `node`: parsed HTML node from rendered Markdown.
///
/// Output:
/// - Static title text when `node` is a heading element.
/// - `None` otherwise.
///
/// Transformation:
/// - Recognizes heading tags and joins descendant static text, trimming
///   whitespace after extraction.
fn markdown_heading_title_from_node(node: &HtmlNode) -> Option<String> {
    let HtmlNode::Element(element) = node else {
        return None;
    };
    if !matches!(
        element.name.as_str(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    ) {
        return None;
    }
    let mut text = String::new();
    collect_static_node_text(&element.children, &mut text);
    let title = text.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// Collects static text from parsed HTML nodes.
///
/// Inputs:
/// - `nodes`: parsed HTML nodes under a heading.
/// - `out`: output buffer for text.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Appends static text recursively through nested elements while ignoring
///   comments, doctypes, and interpolation slots that cannot become a stable
///   route title default.
fn collect_static_node_text(nodes: &[HtmlNode], out: &mut String) {
    for node in nodes {
        match node {
            HtmlNode::Text(text) => out.push_str(text),
            HtmlNode::Element(element) => collect_static_node_text(&element.children, out),
            HtmlNode::Comment(_) | HtmlNode::Doctype(_) | HtmlNode::Slot(_) => {}
        }
    }
}

/// Resolves the route path for one Markdown import.
///
/// Inputs:
/// - `input`: Markdown import with optional page route metadata.
///
/// Output:
/// - Absolute static route path beginning with `/`, or an error string.
///
/// Transformation:
/// - Prefers explicit `@page.route`; otherwise derives the route from the
///   import source path.
fn markdown_static_route_path(input: &SyntaxMarkdownInput) -> Result<String, String> {
    if let Some(route) = &input.metadata.route {
        if route.is_empty() {
            return Err(format!(
                "static Markdown route for `{}` must not be empty",
                input.alias
            ));
        }
        return Ok(route.clone());
    }

    infer_markdown_static_route_path(&input.source_path).or_else(|source_error| {
        infer_markdown_static_route_path_from_resolved_path(
            &input.resolved_path,
            &input.source_path,
        )
        .map_err(|_| source_error)
    })
}

/// Infers a static route from a Markdown import source path.
///
/// Inputs:
/// - `source_path`: import path text from a Markdown content import.
///
/// Output:
/// - Absolute route path such as `/guides/install`.
///
/// Transformation:
/// - Removes leading `./`, optional leading `content/`, and the Markdown
///   suffix. `index` files map to their containing directory route.
fn infer_markdown_static_route_path(source_path: &str) -> Result<String, String> {
    let mut normalized = source_path.replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }

    let mut segments = checked_markdown_source_segments(&normalized)?;
    if segments.first().is_some_and(|segment| segment == "content") {
        segments.remove(0);
    }
    markdown_static_route_path_from_segments(segments, source_path)
}

/// Infers a static route from a resolved Markdown path.
///
/// Inputs:
/// - `resolved_path`: filesystem path resolved from the source import.
/// - `source_path`: original import text used only for diagnostics.
///
/// Output:
/// - Absolute route path such as `/guides/install`.
///
/// Transformation:
/// - Finds the final `content/` directory in the resolved path and derives the
///   route from the path below it. This lets generated projects import
///   `../../content/index.terl.md` from `src/<package>/Site.terl` while keeping
///   route paths project-content-relative.
fn infer_markdown_static_route_path_from_resolved_path(
    resolved_path: &Path,
    source_path: &str,
) -> Result<String, String> {
    let normalized = resolved_path.to_string_lossy().replace('\\', "/");
    let segments = normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let Some(content_index) = segments.iter().rposition(|segment| segment == "content") else {
        return Err(format!(
            "static Markdown source path must resolve under a `content` directory: `{}`",
            source_path
        ));
    };
    let content_segments = segments[content_index + 1..].to_vec();
    checked_markdown_route_segments(content_segments, source_path)
        .and_then(|segments| markdown_static_route_path_from_segments(segments, source_path))
}

/// Converts validated Markdown path segments into a static route path.
///
/// Inputs:
/// - `segments`: Markdown path segments relative to the content route root.
/// - `source_path`: original import text used for diagnostics.
///
/// Output:
/// - Absolute route path beginning with `/`.
///
/// Transformation:
/// - Strips the Markdown suffix, maps final `index` files to their containing
///   route, and joins the remaining segments into a URL path.
fn markdown_static_route_path_from_segments(
    mut segments: Vec<String>,
    source_path: &str,
) -> Result<String, String> {
    if segments.is_empty() {
        return Err(format!(
            "static Markdown source path must include a file name: `{}`",
            source_path
        ));
    }

    let last_index = segments.len() - 1;
    segments[last_index] = strip_markdown_source_suffix(&segments[last_index], source_path)?;
    if segments[last_index].is_empty() {
        return Err(format!(
            "static Markdown source path must include a file stem: `{}`",
            source_path
        ));
    }
    if segments[last_index] == "index" {
        segments.pop();
    }

    if segments.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", segments.join("/")))
    }
}

/// Splits and validates Markdown source path segments.
///
/// Inputs:
/// - `source_path`: normalized import path using `/` separators.
///
/// Output:
/// - Non-empty source path segments, or an error string.
///
/// Transformation:
/// - Rejects current-directory and parent-directory path segments before route
///   inference can turn source paths into output paths.
fn checked_markdown_source_segments(source_path: &str) -> Result<Vec<String>, String> {
    let segments = source_path
        .split('/')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    checked_markdown_route_segments(segments, source_path)
}

/// Validates Markdown route source path segments.
///
/// Inputs:
/// - `segments`: raw source or content-relative resolved path segments.
/// - `source_path`: original source path used for diagnostics.
///
/// Output:
/// - The original segments if every segment is route-safe.
///
/// Transformation:
/// - Rejects empty, current-directory, parent-directory, and drive-style path
///   segments before route inference turns them into public output paths.
fn checked_markdown_route_segments(
    segments: Vec<String>,
    source_path: &str,
) -> Result<Vec<String>, String> {
    if segments.iter().any(|segment| {
        segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
    }) {
        return Err(format!(
            "invalid static Markdown source path `{}`",
            source_path
        ));
    }
    Ok(segments)
}

/// Removes a supported Markdown source suffix from a file name.
///
/// Inputs:
/// - `file_name`: last source path segment.
/// - `source_path`: original source path for diagnostics.
///
/// Output:
/// - File stem without `.terl.md` or `.md`.
///
/// Transformation:
/// - Preserves `.terl.md` as the canonical Terlan Markdown suffix while still
///   accepting plain Markdown imports for compatibility with generic content.
fn strip_markdown_source_suffix(file_name: &str, source_path: &str) -> Result<String, String> {
    file_name
        .strip_suffix(".terl.md")
        .or_else(|| file_name.strip_suffix(".md"))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "static Markdown source path must end with `.terl.md` or `.md`: `{}`",
                source_path
            )
        })
}

/// Parses static route text.
///
/// Inputs:
/// - `raw_text`: source text for a `static route` or `static routes` block.
///
/// Output:
/// - Parsed route declaration shape or an error string.
///
/// Transformation:
/// - Tokenizes route text with quoted-literal and punctuation awareness, then
///   dispatches singular versus block parsing.
pub(crate) fn parse_static_routes_text(raw_text: &str) -> Result<ParsedStaticRoutes, String> {
    let parts = tokenize_static_routes_text(raw_text)?;
    if parts.len() >= 4
        && parts[0] == "static"
        && matches!(parts[1].as_str(), "route" | "routes")
        && parts[2] == "{"
    {
        return parse_static_routes_block_text(raw_text, &parts);
    }

    Ok(ParsedStaticRoutes {
        routes: vec![parse_static_route_parts_text(raw_text, &parts)?],
        is_block: false,
    })
}

/// Parses a singular static route declaration.
///
/// Inputs:
/// - `raw_text`: original route text for error reporting.
/// - `parts`: whitespace-split route tokens.
///
/// Output:
/// - One static route or an error string.
///
/// Transformation:
/// - Validates the expected token shape and normalizes quoted path literals.
fn parse_static_route_parts_text(raw_text: &str, parts: &[String]) -> Result<StaticRoute, String> {
    let parts = strip_optional_trailing_dot(parts);
    if parts.len() != 7
        || parts[0] != "static"
        || parts[1] != "route"
        || parts[3] != "->"
        || parts[5] != "("
        || parts[6] != ")"
    {
        return Err(format!("invalid static route declaration `{}`", raw_text));
    }

    let path = normalize_static_route_path_literal(&parts[2]);
    if !path.starts_with('/') {
        return Err(format!("static route path must start with `/`: `{}`", path));
    }

    Ok(StaticRoute {
        path,
        handler: parts[4].to_owned(),
    })
}

/// Parses a static routes block declaration.
///
/// Inputs:
/// - `raw_text`: original block text for error reporting.
/// - `parts`: whitespace-split block tokens.
///
/// Output:
/// - Parsed route block or an error string.
///
/// Transformation:
/// - Walks repeated path/handler entries inside the surrounding braces.
fn parse_static_routes_block_text(
    raw_text: &str,
    parts: &[String],
) -> Result<ParsedStaticRoutes, String> {
    let parts = strip_optional_trailing_dot(parts);
    if parts.last().is_none_or(|part| part != "}") {
        return Err(format!("invalid static routes declaration `{}`", raw_text));
    }

    let mut routes = Vec::new();
    let mut index = 3;
    while index < parts.len() - 1 {
        if index + 5 >= parts.len() {
            return Err(format!("invalid static routes declaration `{}`", raw_text));
        }
        if parts[index + 1] != "->"
            || parts[index + 3] != "("
            || parts[index + 4] != ")"
            || parts[index + 5] != "."
        {
            return Err(format!("invalid static routes declaration `{}`", raw_text));
        }

        let path = normalize_static_route_path_literal(&parts[index]);
        if !path.starts_with('/') {
            return Err(format!("static route path must start with `/`: `{}`", path));
        }

        routes.push(StaticRoute {
            path,
            handler: parts[index + 2].to_owned(),
        });
        index += 6;
    }

    Ok(ParsedStaticRoutes {
        routes,
        is_block: true,
    })
}

/// Tokenizes a static route declaration.
///
/// Inputs:
/// - `raw_text`: parser-preserved `static route` or `static routes` text.
///
/// Output:
/// - Route grammar tokens or an error string for unterminated quoted text.
///
/// Transformation:
/// - Splits route declarations on whitespace and route punctuation while
///   preserving quoted path literals as single tokens.
fn tokenize_static_routes_text(raw_text: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let chars = raw_text.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            let (token, next_index) = tokenize_static_quoted_literal(raw_text, &chars, index)?;
            tokens.push(token);
            index = next_index;
            continue;
        }
        if ch == '-' && chars.get(index + 1) == Some(&'>') {
            tokens.push("->".to_string());
            index += 2;
            continue;
        }
        if matches!(ch, '{' | '}' | '(' | ')' | '.') {
            tokens.push(ch.to_string());
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() {
            let current = chars[index];
            if current.is_whitespace()
                || matches!(current, '"' | '\'' | '{' | '}' | '(' | ')' | '.')
                || (current == '-' && chars.get(index + 1) == Some(&'>'))
            {
                break;
            }
            index += 1;
        }
        tokens.push(chars[start..index].iter().collect());
    }

    Ok(tokens)
}

/// Tokenizes one quoted static route literal.
///
/// Inputs:
/// - `raw_text`: original route text for diagnostics.
/// - `chars`: route text split into Unicode scalar values.
/// - `start`: index of the opening quote.
///
/// Output:
/// - Quoted literal token and the next unread index.
///
/// Transformation:
/// - Copies characters through the matching closing quote, preserving escaped
///   characters so later normalization can decode path text deterministically.
fn tokenize_static_quoted_literal(
    raw_text: &str,
    chars: &[char],
    start: usize,
) -> Result<(String, usize), String> {
    let quote = chars[start];
    let mut index = start + 1;
    let mut token = String::new();
    token.push(quote);
    while index < chars.len() {
        let current = chars[index];
        token.push(current);
        if current == '\\' {
            if let Some(next) = chars.get(index + 1) {
                token.push(*next);
                index += 2;
                continue;
            }
        }
        index += 1;
        if current == quote {
            return Ok((token, index));
        }
    }

    Err(format!("unterminated static route literal `{}`", raw_text))
}

/// Removes an optional declaration-terminating dot token.
///
/// Inputs:
/// - `parts`: route tokens.
///
/// Output:
/// - A borrowed token slice without a final `.` token when present.
///
/// Transformation:
/// - Allows callers to parse parser-preserved text with or without the
///   top-level declaration terminator.
fn strip_optional_trailing_dot(parts: &[String]) -> &[String] {
    if parts.last().is_some_and(|part| part == ".") {
        &parts[..parts.len() - 1]
    } else {
        parts
    }
}

/// Rejects duplicate static route paths.
///
/// Inputs:
/// - `routes`: parsed static routes.
///
/// Output:
/// - `Ok(())` when all paths are unique, otherwise an error string.
///
/// Transformation:
/// - Tracks seen paths in a set and reports the first duplicate.
fn reject_duplicate_static_routes(routes: &[StaticRoute]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for route in routes {
        if !seen.insert(route.path.clone()) {
            return Err(format!("duplicate static route `{}`", route.path));
        }
    }
    Ok(())
}

/// Rejects duplicate Markdown static route paths.
///
/// Inputs:
/// - `routes`: discovered Markdown content routes.
///
/// Output:
/// - `Ok(())` when all paths are unique, otherwise an error string.
///
/// Transformation:
/// - Tracks route paths separately from aliases so duplicate content URLs are
///   rejected regardless of import names.
fn reject_duplicate_markdown_static_routes(routes: &[StaticMarkdownRoute]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for route in routes {
        if !seen.insert(route.path.clone()) {
            return Err(format!("duplicate static Markdown route `{}`", route.path));
        }
    }
    Ok(())
}

/// Rejects path collisions between handler routes and Markdown content routes.
///
/// Inputs:
/// - `routes`: explicit `static route` handler routes.
/// - `markdown_routes`: Markdown content routes inferred from imports.
///
/// Output:
/// - `Ok(())` when every emitted route path has a single source, otherwise an
///   error string naming the colliding path.
///
/// Transformation:
/// - Builds a set of explicit route paths and checks Markdown routes against
///   that set before static output writes files.
pub(crate) fn reject_static_route_path_collisions(
    routes: &[StaticRoute],
    markdown_routes: &[StaticMarkdownRoute],
) -> Result<(), String> {
    let explicit_paths = routes
        .iter()
        .map(|route| route.path.as_str())
        .collect::<BTreeSet<_>>();
    for route in markdown_routes {
        if explicit_paths.contains(route.path.as_str()) {
            return Err(format!(
                "static route `{}` is also produced by Markdown import `{}`",
                route.path, route.alias
            ));
        }
    }
    Ok(())
}

/// Normalizes a static route path literal.
///
/// Inputs:
/// - `path`: raw path token, possibly single- or double-quoted.
///
/// Output:
/// - Path text without matching surrounding quotes.
///
/// Transformation:
/// - Removes one matching pair of quote delimiters when present.
fn normalize_static_route_path_literal(path: &str) -> String {
    path.strip_prefix('"')
        .and_then(|path| path.strip_suffix('"'))
        .or_else(|| {
            path.strip_prefix('\'')
                .and_then(|path| path.strip_suffix('\''))
        })
        .unwrap_or(path)
        .to_owned()
}

/// Validates formal syntax-output static route handlers.
///
/// Inputs:
/// - `module`: formal syntax output module.
/// - `routes`: static routes to validate.
///
/// Output:
/// - `Ok(())` when every route targets a zero-argument template HTML handler.
/// - `Err(String)` naming the first invalid handler.
///
/// Transformation:
/// - Resolves route handlers against syntax-output function declarations and
///   checks arity plus return type.
pub(crate) fn validate_syntax_static_route_handlers(
    module: &SyntaxModuleOutput,
    routes: &[StaticRoute],
) -> Result<(), String> {
    for route in routes {
        let Some((params, return_type)) =
            module
                .declarations
                .iter()
                .find_map(|declaration| match &declaration.payload {
                    SyntaxDeclarationPayload::Function {
                        name,
                        params,
                        return_type,
                        ..
                    } if name == &route.handler => Some((params, return_type)),
                    _ => None,
                })
        else {
            return Err(format!(
                "static route `{}` references unknown handler `{}`",
                route.path, route.handler
            ));
        };

        if !params.is_empty() {
            return Err(format!(
                "static route `{}` handler `{}` must not require arguments",
                route.path, route.handler
            ));
        }
        if !is_static_html_return_type(&return_type.text) {
            return Err(format!(
                "static route `{}` handler `{}` must return Template.Html",
                route.path, route.handler
            ));
        }
    }

    Ok(())
}

/// Converts a route path into a relative output path.
///
/// Inputs:
/// - `route_path`: URL path from a static route declaration.
///
/// Output:
/// - Relative output path ending in `index.html`, or an error string.
///
/// Transformation:
/// - Maps `/` to `index.html`; otherwise maps path segments to directories and
///   rejects empty, current-directory, or parent-directory segments.
pub(crate) fn static_route_output_path(route_path: &str) -> Result<PathBuf, String> {
    if route_path == "/" {
        return Ok(PathBuf::from("index.html"));
    }
    if !route_path.starts_with('/') {
        return Err(format!(
            "static route path must start with `/`: `{}`",
            route_path
        ));
    }

    let mut output = PathBuf::new();
    for segment in route_path.trim_matches('/').split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!("invalid static route path `{}`", route_path));
        }
        output.push(segment);
    }
    output.push("index.html");
    Ok(output)
}

/// Returns whether a type annotation is an accepted static HTML route type.
///
/// Inputs:
/// - `type_text`: source spelling of a return type.
///
/// Output:
/// - `true` when the normalized type text is a supported template HTML type.
///
/// Transformation:
/// - Removes whitespace before comparing the current public spelling and the
///   older internal spelling still accepted by static-site fixtures.
pub(crate) fn is_static_html_return_type(type_text: &str) -> bool {
    let normalized = type_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "Html" | "Template.Html" | "std.template.Template.Html" | "Html[Never]"
    )
}

#[cfg(test)]
#[path = "routes_test.rs"]
mod routes_test;
