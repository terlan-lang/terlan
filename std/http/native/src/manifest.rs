//! HTTP package route metadata shared by build and serving hosts.

use serde::{Deserialize, Serialize};

mod namespace;
mod validation;
pub use namespace::validate_route_namespace;
pub use validation::*;

#[cfg(test)]
mod namespace_test;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceSpan {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HandlerRoute {
    pub method: String,
    pub route: String,
    pub module: String,
    pub function: String,
    pub arity: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSpan>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WebSocketRoute {
    #[serde(default)]
    pub module: String,
    pub route: String,
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSpan>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SseRoute {
    pub module: String,
    pub route: String,
    pub source: SourceSpan,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ErrorHandler {
    pub module: String,
    pub function: String,
    pub arity: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ResponseHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct StaticResponse {
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub function: String,
    #[serde(default)]
    pub arity: usize,
    pub method: String,
    pub route: String,
    pub status: u16,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<ResponseHeader>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSpan>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileResponse {
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub function: String,
    #[serde(default)]
    pub arity: usize,
    pub method: String,
    pub route: String,
    pub path: String,
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSpan>,
}
