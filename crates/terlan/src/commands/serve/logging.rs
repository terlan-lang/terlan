use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use super::handler::{HandlerLogIdentity, WebPackageSourceSpan};

/// Local request id counter for `terlc serve`.
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Allocates the next local request id.
///
/// Inputs:
/// - No explicit input; uses the process-local atomic counter.
///
/// Output:
/// - Monotonically increasing request id for this `terlc serve` process.
///
/// Transformation:
/// - Increments an atomic counter with relaxed ordering because ids only need
///   uniqueness for local log correlation, not memory synchronization.
pub(super) fn next_request_id() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Writes one dynamic-handler request log line.
///
/// Inputs:
/// - Request id, concrete request method/path, matched handler identity,
///   response status, and elapsed handler duration.
///
/// Output:
/// - One line written to stderr.
///
/// Transformation:
/// - Delegates formatting to `render_handler_log_line` so the log contract
///   remains testable without binding sockets or capturing stderr.
pub(super) fn log_handler_result(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    identity: &HandlerLogIdentity<'_>,
    status: u16,
    duration_ms: u128,
) {
    eprintln!(
        "{}",
        render_handler_log_line(
            request_id,
            build_id,
            request_method,
            request_path,
            identity,
            status,
            duration_ms,
        )
    );
}

/// Renders one local dynamic-handler log line.
///
/// Inputs:
/// - `request_id`: process-local request id.
/// - `build_id`: deterministic browser package build id.
/// - `request_method`: concrete HTTP request method.
/// - `request_path`: concrete URL path.
/// - `identity`: matched route and Terlan handler metadata.
/// - `status`: emitted HTTP response status.
/// - `duration_ms`: elapsed handler duration in milliseconds.
///
/// Output:
/// - Stable human-readable local development log line.
///
/// Transformation:
/// - Combines runtime request metadata with compiler-generated handler
///   identity so future diagnostics and Terlan Cloud observability have a
///   source-shaped local precedent.
pub(super) fn render_handler_log_line(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    identity: &HandlerLogIdentity<'_>,
    status: u16,
    duration_ms: u128,
) -> String {
    let mut line = format!(
        "terlc serve request_id={request_id} build_id={build_id} method={request_method} path={request_path} route_method={} route={} handler={}.{} status={status} duration_ms={duration_ms}",
        identity.method, identity.route, identity.module, identity.function
    );
    append_source_span(&mut line, identity.source);
    line
}

/// Writes one static-file request log line.
///
/// Inputs:
/// - Request id, concrete request method/path, selected filesystem response
///   path, response status, and elapsed duration.
///
/// Output:
/// - One line written to stderr.
///
/// Transformation:
/// - Delegates formatting to `render_static_log_line` so static response logs
///   share the source-aware local development contract without coupling tests
///   to stderr.
pub(super) fn log_static_result(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    response_path: &Path,
    status: u16,
    duration_ms: u128,
) {
    eprintln!(
        "{}",
        render_static_log_line(
            request_id,
            build_id,
            request_method,
            request_path,
            response_path,
            status,
            duration_ms,
        )
    );
}

/// Writes one static-route response log line.
///
/// Inputs:
/// - Request id, build id, concrete request method/path, matched route method
///   and pattern, response status, and elapsed duration.
///
/// Output:
/// - One line written to stderr.
///
/// Transformation:
/// - Keeps manifest-cached static responses visible in local logs without
///   pretending they were served from a filesystem asset path.
pub(super) fn log_static_route_result(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    route_method: &str,
    route: &str,
    source: Option<&WebPackageSourceSpan>,
    status: u16,
    duration_ms: u128,
) {
    eprintln!(
        "{}",
        render_static_route_log_line(
            request_id,
            build_id,
            request_method,
            request_path,
            route_method,
            route,
            source,
            status,
            duration_ms,
        )
    );
}

/// Writes one file-route response log line.
///
/// Inputs:
/// - Request id, build id, concrete request method/path, matched route method
///   and pattern, selected package file, response status, and elapsed duration.
///
/// Output:
/// - One line written to stderr.
///
/// Transformation:
/// - Keeps route-backed file responses distinct from generic static-file
///   fallback logs while preserving source-aware route metadata.
pub(super) fn log_file_route_result(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    route_method: &str,
    route: &str,
    response_path: &Path,
    source: Option<&WebPackageSourceSpan>,
    status: u16,
    duration_ms: u128,
) {
    eprintln!(
        "{}",
        render_file_route_log_line(
            request_id,
            build_id,
            request_method,
            request_path,
            route_method,
            route,
            response_path,
            source,
            status,
            duration_ms,
        )
    );
}

/// Renders one local static-file log line.
///
/// Inputs:
/// - `request_id`: process-local request id.
/// - `build_id`: deterministic browser package build id.
/// - `request_method`: concrete HTTP request method.
/// - `request_path`: concrete URL path.
/// - `response_path`: package file path selected for the response.
/// - `status`: emitted HTTP response status.
/// - `duration_ms`: elapsed static response duration in milliseconds.
///
/// Output:
/// - Stable human-readable local development log line.
///
/// Transformation:
/// - Captures static asset serving with the same request-id/status/duration
///   fields used by dynamic handler logs while adding the selected asset path.
pub(super) fn render_static_log_line(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    response_path: &Path,
    status: u16,
    duration_ms: u128,
) -> String {
    format!(
        "terlc serve request_id={request_id} build_id={build_id} method={request_method} path={request_path} static={} status={status} duration_ms={duration_ms}",
        response_path.display()
    )
}

/// Renders one local static-route response log line.
///
/// Inputs:
/// - `request_id`: process-local request id.
/// - `build_id`: deterministic browser package build id.
/// - `request_method`: concrete HTTP request method.
/// - `request_path`: concrete URL path.
/// - `route_method`: manifest route method selected for the response.
/// - `route`: manifest route pattern selected for the response.
/// - `source`: optional source span for the generated static route row.
/// - `status`: emitted HTTP response status.
/// - `duration_ms`: elapsed response duration in milliseconds.
///
/// Output:
/// - Stable human-readable local development log line.
///
/// Transformation:
/// - Captures compiler-cached static responses with the same request-id,
///   build-id, status, and duration fields used by handler and file logs.
pub(super) fn render_static_route_log_line(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    route_method: &str,
    route: &str,
    source: Option<&WebPackageSourceSpan>,
    status: u16,
    duration_ms: u128,
) -> String {
    let mut line = format!(
        "terlc serve request_id={request_id} build_id={build_id} method={request_method} path={request_path} static_route_method={route_method} static_route={route} status={status} duration_ms={duration_ms}"
    );
    append_source_span(&mut line, source);
    line
}

/// Renders one local file-route response log line.
///
/// Inputs:
/// - `request_id`: process-local request id.
/// - `build_id`: deterministic browser package build id.
/// - `request_method`: concrete HTTP request method.
/// - `request_path`: concrete URL path.
/// - `route_method`: manifest route method selected for the response.
/// - `route`: manifest route pattern selected for the response.
/// - `response_path`: package file path streamed for the response.
/// - `source`: optional source span for the generated file route row.
/// - `status`: emitted HTTP response status.
/// - `duration_ms`: elapsed response duration in milliseconds.
///
/// Output:
/// - Stable human-readable local development log line.
///
/// Transformation:
/// - Captures route-backed file responses with both route identity and selected
///   package file path for local debugging.
pub(super) fn render_file_route_log_line(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    route_method: &str,
    route: &str,
    response_path: &Path,
    source: Option<&WebPackageSourceSpan>,
    status: u16,
    duration_ms: u128,
) -> String {
    let mut line = format!(
        "terlc serve request_id={request_id} build_id={build_id} method={request_method} path={request_path} file_route_method={route_method} file_route={route} file={} status={status} duration_ms={duration_ms}",
        response_path.display()
    );
    append_source_span(&mut line, source);
    line
}

/// Appends optional source metadata to one serve log line.
///
/// Inputs:
/// - `line`: mutable log line under construction.
/// - `source`: optional validated manifest source span.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Adds the same `source=path:line:column` suffix used by dynamic-handler
///   logs so route observability remains uniform across manifest row kinds.
fn append_source_span(line: &mut String, source: Option<&WebPackageSourceSpan>) {
    if let Some(source) = source {
        line.push_str(&format!(
            " source={}:{}:{}",
            source.path, source.line, source.column
        ));
    }
}

/// Renders a browser-readable development handler error page.
///
/// Inputs:
/// - `request_id`: process-local request id.
/// - `build_id`: deterministic browser package build id.
/// - `request_method`: concrete HTTP request method.
/// - `request_path`: concrete URL path.
/// - `identity`: matched route and Terlan handler metadata.
/// - `message`: backend failure message.
///
/// Output:
/// - HTML document body for the development 502 response.
///
/// Transformation:
/// - Converts backend handler failures into a source-aware page while escaping
///   every dynamic value before embedding it into HTML.
pub(super) fn render_dev_error_page(
    request_id: u64,
    build_id: &str,
    request_method: &str,
    request_path: &str,
    identity: &HandlerLogIdentity<'_>,
    message: &str,
) -> String {
    format!(
        concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<title>Terlan handler error</title>",
            "<style>body{{font-family:system-ui,sans-serif;margin:2rem;line-height:1.45}}",
            "code,pre{{background:#f5f5f5;border-radius:4px;padding:.2rem .35rem}}",
            "pre{{padding:1rem;white-space:pre-wrap}}</style>",
            "</head><body>",
            "<h1>Terlan handler error</h1>",
            "<p><strong>Code:</strong> <code>serve_handler.execution_failed</code></p>",
            "<p><strong>Message:</strong> Handler execution failed.</p>",
            "<p><strong>Request:</strong> <code>{} {}</code></p>",
            "<p><strong>Route:</strong> <code>{} {}</code></p>",
            "<p><strong>Handler:</strong> <code>{}.{}</code></p>",
            "{}",
            "<p><strong>Request id:</strong> <code>{}</code></p>",
            "<p><strong>Build id:</strong> <code>{}</code></p>",
            "<h2>Backend error</h2><pre>{}</pre>",
            "</body></html>"
        ),
        escape_serve_html_text(request_method),
        escape_serve_html_text(request_path),
        escape_serve_html_text(identity.method),
        escape_serve_html_text(identity.route),
        escape_serve_html_text(identity.module),
        escape_serve_html_text(identity.function),
        render_source_span_html(identity),
        request_id,
        escape_serve_html_text(build_id),
        escape_serve_html_text(message),
    )
}

/// Renders optional handler source span metadata for dev error pages.
///
/// Inputs:
/// - `identity`: handler identity with optional source metadata.
///
/// Output:
/// - Empty string when no source is present.
/// - HTML paragraph containing escaped source path and one-based position.
///
/// Transformation:
/// - Keeps the optional source display isolated so the main error-page template
///   remains stable for manifests that do not yet emit source spans.
fn render_source_span_html(identity: &HandlerLogIdentity<'_>) -> String {
    let Some(source) = identity.source else {
        return String::new();
    };
    format!(
        "<p><strong>Source:</strong> <code>{}:{}:{}</code></p>",
        escape_serve_html_text(&source.path),
        source.line,
        source.column
    )
}

/// Escapes text for local development HTML output.
///
/// Inputs:
/// - `input`: untrusted text from request, route, handler, or backend error
///   metadata.
///
/// Output:
/// - HTML text-safe string.
///
/// Transformation:
/// - Replaces the five characters that can change HTML text interpretation.
fn escape_serve_html_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
