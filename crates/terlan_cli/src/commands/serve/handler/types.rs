use serde::Deserialize;

/// Source span metadata for generated web package manifest entries.
///
/// Inputs:
/// - Deserialized from optional handler manifest source metadata.
///
/// Output:
/// - Project-relative source path plus one-based line and column values.
///
/// Transformation:
/// - Preserves compiler-generated source identity for local logs, development
///   error pages, and future cloud observability without requiring the local
///   server to parse Terlan source files.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageSourceSpan {
    pub(in crate::commands::serve) path: String,
    pub(in crate::commands::serve) line: usize,
    pub(in crate::commands::serve) column: usize,
}

/// One dynamic handler entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Route method/path and Terlan module/function identity reserved for
///   BEAM-backed handler dispatch.
///
/// Transformation:
/// - Keeps dynamic routes declarative in the package manifest so the local
///   server does not hard-code application route behavior.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageHandler {
    pub(in crate::commands::serve) method: String,
    pub(in crate::commands::serve) route: String,
    pub(in crate::commands::serve) module: String,
    pub(in crate::commands::serve) function: String,
    pub(in crate::commands::serve) arity: usize,
    #[serde(default)]
    pub(in crate::commands::serve) source: Option<WebPackageSourceSpan>,
}

/// One WebSocket route entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Route path and runtime protocol identity reserved for WebSocket upgrade
///   handling.
///
/// Transformation:
/// - Keeps long-lived socket routes declarative alongside HTTP handlers while
///   the server owns protocol upgrade and connection lifecycle mechanics.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageWebSocket {
    pub(in crate::commands::serve) route: String,
    pub(in crate::commands::serve) protocol: String,
    #[serde(default)]
    pub(in crate::commands::serve) source: Option<WebPackageSourceSpan>,
}

/// Router-level error handler entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Terlan module/function identity reserved for source-visible error
///   rendering callbacks.
///
/// Transformation:
/// - Keeps error handler metadata separate from normal route handlers so route
///   precedence and fallback matching stay unchanged.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageErrorHandler {
    pub(in crate::commands::serve) module: String,
    pub(in crate::commands::serve) function: String,
    pub(in crate::commands::serve) arity: usize,
}

/// One validated static response header entry.
///
/// Inputs:
/// - Deserialized from a static response manifest row.
///
/// Output:
/// - Header name/value metadata accepted by `terlc serve`.
///
/// Transformation:
/// - Keeps static response headers readable in manifest JSON while the HTTP
///   writer continues to use the same tuple form as dynamic responses.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageResponseHeader {
    pub(in crate::commands::serve) name: String,
    pub(in crate::commands::serve) value: String,
}

/// One cacheable static response entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Route method/path and literal response payload.
///
/// Transformation:
/// - Represents compiler-discovered constant responses separately from dynamic
///   BEAM-backed handlers so `terlc serve` can validate and later serve them
///   without dispatching through generated BEAM code.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageStaticResponse {
    pub(in crate::commands::serve) method: String,
    pub(in crate::commands::serve) route: String,
    pub(in crate::commands::serve) status: u16,
    pub(in crate::commands::serve) content_type: String,
    #[serde(default)]
    pub(in crate::commands::serve) headers: Vec<WebPackageResponseHeader>,
    pub(in crate::commands::serve) body: String,
    #[serde(default)]
    pub(in crate::commands::serve) source: Option<WebPackageSourceSpan>,
}

/// One file response entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Route method/path and package-relative file response metadata.
///
/// Transformation:
/// - Represents compiler-discovered file responses separately from generic
///   asset serving so typed routes can stream files through the Rust server
///   without dispatching through BEAM handler code.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::commands::serve) struct WebPackageFileResponse {
    pub(in crate::commands::serve) method: String,
    pub(in crate::commands::serve) route: String,
    pub(in crate::commands::serve) path: String,
    pub(in crate::commands::serve) status: u16,
    #[serde(default)]
    pub(in crate::commands::serve) content_type: Option<String>,
    #[serde(default)]
    pub(in crate::commands::serve) source: Option<WebPackageSourceSpan>,
}
