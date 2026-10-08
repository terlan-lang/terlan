//! Shared HTTP package manifest contract; no host-owned wire schema.
#[cfg(test)]
pub(in crate::commands::serve) use terlan_http_native::manifest::SourceSpan as WebPackageSourceSpan;
pub(in crate::commands::serve) use terlan_http_native::manifest::{
    ErrorHandler as WebPackageErrorHandler, FileResponse as WebPackageFileResponse,
    HandlerRoute as WebPackageHandler, ResponseHeader as WebPackageResponseHeader,
    SseRoute as WebPackageSse, StaticResponse as WebPackageStaticResponse,
    WebSocketRoute as WebPackageWebSocket,
};
