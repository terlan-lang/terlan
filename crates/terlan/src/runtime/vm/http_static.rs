#![allow(dead_code)]

//! Static asset and response body metadata for VM HTTP.

use std::collections::HashMap;
use std::path::{Component, Path};

use super::sse::VmSseEvent;
use crate::terlan_native::http::content_type_for_path;

mod http1_stream;
mod range;
mod stream;

pub(crate) use http1_stream::VmHttp1ResponseStream;
#[cfg(test)]
pub(crate) use http1_stream::VmHttp1StreamTcpFlush;
pub(crate) use range::VmHttpByteRange;

#[cfg(test)]
#[path = "http_static/http1_stream_test.rs"]
mod http1_stream_test;
#[cfg(test)]
#[path = "http_static_test.rs"]
mod http_static_test;
#[cfg(test)]
#[path = "http_static/stream_test.rs"]
mod stream_test;

/// Static asset table failure with stable typed variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpStaticError {
    InvalidRoute,
    InvalidAssetPath,
    DuplicateRoute,
    AssetTooLarge,
    AssetNotFound,
    InvalidResponse,
    InvalidSseEvent,
    InvalidStreamLimit,
    InvalidRange,
    UnsatisfiableRange,
    StreamBackpressure,
    StreamClosed,
    StreamAborted,
    InvalidStreamChunk,
    StreamTransportClosed,
    StreamTransportCancelled,
    StreamTransportInvalid,
    InvalidStreamResponse,
    UnsupportedStreaming,
}

/// One manifest-declared static asset.
///
/// Inputs: route path, package path, bytes, and optional metadata.
/// Output: normalized static asset entry.
/// Transformation: keeps static asset serving deterministic and independent
/// from filesystem watchers, live sockets, or host async runtimes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpStaticAsset {
    route_path: String,
    package_path: String,
    content_type: String,
    bytes: Vec<u8>,
    cache_control: String,
    fingerprint: Option<String>,
}

impl VmHttpStaticAsset {
    /// Returns the HTTP route path used for lookup.
    pub(crate) fn route_path(&self) -> &str {
        &self.route_path
    }

    /// Returns the package-relative source path.
    pub(crate) fn package_path(&self) -> &str {
        &self.package_path
    }

    /// Returns the response content type.
    pub(crate) fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Returns borrowed asset bytes.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns response cache-control metadata.
    pub(crate) fn cache_control(&self) -> &str {
        &self.cache_control
    }

    /// Returns immutable asset fingerprint metadata when present.
    pub(crate) fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }
}

/// Manifest entry accepted by `VmHttpStaticAssetTable`.
///
/// Inputs: route path, package path, content bytes, content-type override,
/// cache-control override, and fingerprint metadata.
/// Output: insertable manifest row.
/// Transformation: separates manifest normalization from route lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpStaticManifestEntry {
    pub(crate) route_path: String,
    pub(crate) package_path: String,
    pub(crate) bytes: Vec<u8>,
    pub(crate) content_type: Option<String>,
    pub(crate) cache_control: Option<String>,
    pub(crate) fingerprint: Option<String>,
}

/// VM-owned static asset table.
///
/// Inputs: manifest entries and configured maximum asset size.
/// Output: route-indexed static assets with inferred response metadata.
/// Transformation: gives the HTTP router a deterministic in-memory fixture
/// before live package asset loading is wired.
#[derive(Debug)]
pub(crate) struct VmHttpStaticAssetTable {
    max_asset_bytes: usize,
    assets: HashMap<String, VmHttpStaticAsset>,
}

impl VmHttpStaticAssetTable {
    /// Creates an empty static asset table with a maximum asset byte size.
    pub(crate) fn new(max_asset_bytes: usize) -> Result<Self, VmHttpStaticError> {
        if max_asset_bytes == 0 {
            return Err(VmHttpStaticError::AssetTooLarge);
        }
        Ok(Self {
            max_asset_bytes,
            assets: HashMap::new(),
        })
    }

    /// Inserts one normalized manifest entry.
    pub(crate) fn insert(
        &mut self,
        entry: VmHttpStaticManifestEntry,
    ) -> Result<(), VmHttpStaticError> {
        validate_route_path(&entry.route_path)?;
        validate_package_path(&entry.package_path)?;
        if entry.bytes.len() > self.max_asset_bytes {
            return Err(VmHttpStaticError::AssetTooLarge);
        }
        if self.assets.contains_key(&entry.route_path) {
            return Err(VmHttpStaticError::DuplicateRoute);
        }

        let content_type = entry
            .content_type
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| content_type_for_path(Path::new(&entry.package_path)));
        let cache_control = entry.cache_control.unwrap_or_else(|| {
            if entry.fingerprint.is_some() {
                "public, max-age=31536000, immutable".to_string()
            } else {
                "no-cache".to_string()
            }
        });
        let asset = VmHttpStaticAsset {
            route_path: entry.route_path.clone(),
            package_path: entry.package_path,
            content_type,
            bytes: entry.bytes,
            cache_control,
            fingerprint: entry.fingerprint,
        };
        self.assets.insert(entry.route_path, asset);
        Ok(())
    }

    /// Inserts every manifest entry as one atomic batch.
    pub(crate) fn insert_manifest(
        &mut self,
        entries: Vec<VmHttpStaticManifestEntry>,
    ) -> Result<(), VmHttpStaticError> {
        let original = self.assets.clone();
        for entry in entries {
            if let Err(error) = self.insert(entry) {
                self.assets = original;
                return Err(error);
            }
        }
        Ok(())
    }

    /// Looks up one static asset by route path.
    pub(crate) fn lookup(&self, route_path: &str) -> Result<&VmHttpStaticAsset, VmHttpStaticError> {
        self.assets
            .get(route_path)
            .ok_or(VmHttpStaticError::AssetNotFound)
    }

    /// Looks up one manifest asset by its package-relative source path.
    ///
    /// When one package asset is mounted at multiple routes, the
    /// lexicographically smallest route wins so response materialization does
    /// not depend on randomized `HashMap` iteration order.
    pub(crate) fn lookup_package_path(
        &self,
        package_path: &str,
    ) -> Result<&VmHttpStaticAsset, VmHttpStaticError> {
        self.assets
            .values()
            .filter(|asset| asset.package_path() == package_path)
            .min_by(|left, right| left.route_path().cmp(right.route_path()))
            .ok_or(VmHttpStaticError::AssetNotFound)
    }

    /// Resolves a route and builds its deterministic single-range response.
    pub(crate) fn range_http_response(
        &self,
        route_path: &str,
        range: VmHttpByteRange,
    ) -> Result<::http::Response<Vec<u8>>, VmHttpStaticError> {
        self.lookup(route_path)?.range_http_response(range)
    }

    /// Returns the number of installed static assets.
    pub(crate) fn len(&self) -> usize {
        self.assets.len()
    }
}

/// VM HTTP response body strategy.
///
/// Inputs: handler or static route response data.
/// Output: explicit response body mode.
/// Transformation: avoids implicit conversion from arbitrary values into HTTP
/// bytes and keeps streaming as a typed VM contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpResponseBody {
    Empty,
    Text(String),
    Binary(Vec<u8>),
    StaticAsset(VmHttpStaticAsset),
    SseEventStream(Vec<VmSseEvent>),
    Stream(VmHttpStreamPlan),
}

/// Static stream emission plan.
///
/// Inputs: maximum chunk size and pending write count.
/// Output: bounded stream plan for later VM scheduling.
/// Transformation: records backpressure limits without exposing scheduler
/// internals or host async primitives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpStreamPlan {
    chunk_size: usize,
    max_pending_writes: usize,
}

impl VmHttpStreamPlan {
    /// Creates a stream plan with explicit non-zero limits.
    pub(crate) fn new(
        chunk_size: usize,
        max_pending_writes: usize,
    ) -> Result<Self, VmHttpStaticError> {
        if chunk_size == 0 || max_pending_writes == 0 {
            return Err(VmHttpStaticError::InvalidStreamLimit);
        }
        Ok(Self {
            chunk_size,
            max_pending_writes,
        })
    }

    /// Returns maximum emitted chunk size.
    pub(crate) fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Returns maximum queued writes.
    pub(crate) fn max_pending_writes(&self) -> usize {
        self.max_pending_writes
    }

    /// Returns the stable unsupported-streaming outcome for non-VM backends.
    pub(crate) fn unsupported_backend() -> VmHttpStaticError {
        VmHttpStaticError::UnsupportedStreaming
    }

    /// Opens a pollable VM-owned response stream using this bounded plan.
    pub(crate) fn open_stream(&self) -> stream::VmHttpResponseStream {
        stream::VmHttpResponseStream::new(self.clone())
    }

    /// Opens a chunk-framed HTTP/1 response stream over the VM TCP lane.
    pub(crate) fn open_http1_stream(
        &self,
        response: ::http::Response<()>,
        close_connection: bool,
    ) -> Result<http1_stream::VmHttp1ResponseStream, VmHttpStaticError> {
        http1_stream::VmHttp1ResponseStream::new(self.clone(), response, close_connection)
    }
}

impl VmHttpResponseBody {
    /// Converts an explicit VM body mode into a byte HTTP response.
    ///
    /// Inputs:
    /// - `self`: explicit VM response body mode.
    /// - `status`: validated HTTP status for the response.
    ///
    /// Output:
    /// - Maintained `http::Response<Vec<u8>>` with deterministic body headers,
    ///   or a typed static response error.
    ///
    /// Transformation:
    /// - Assigns content metadata from the body mode while rejecting stream
    ///   bodies until scheduler-backed chunk emission is implemented.
    pub(crate) fn into_http_response(
        self,
        status: ::http::StatusCode,
    ) -> Result<::http::Response<Vec<u8>>, VmHttpStaticError> {
        match self {
            Self::Empty => build_response(status, None, None, Vec::new()),
            Self::Text(body) => build_response(
                status,
                Some("text/plain; charset=utf-8"),
                None,
                body.into_bytes(),
            ),
            Self::Binary(body) => {
                build_response(status, Some("application/octet-stream"), None, body)
            }
            Self::StaticAsset(asset) => build_response(
                status,
                Some(asset.content_type()),
                Some(asset.cache_control()),
                asset.bytes().to_vec(),
            ),
            Self::SseEventStream(events) => build_response(
                status,
                Some("text/event-stream; charset=utf-8"),
                Some("no-cache"),
                encode_sse_events(events)?,
            ),
            Self::Stream(_) => Err(VmHttpStaticError::UnsupportedStreaming),
        }
    }
}

fn encode_sse_events(events: Vec<VmSseEvent>) -> Result<Vec<u8>, VmHttpStaticError> {
    let mut body = Vec::new();
    for event in events {
        let bytes = event
            .encode()
            .map_err(|_| VmHttpStaticError::InvalidSseEvent)?;
        body.extend_from_slice(&bytes);
    }
    Ok(body)
}

fn build_response(
    status: ::http::StatusCode,
    content_type: Option<&str>,
    cache_control: Option<&str>,
    body: Vec<u8>,
) -> Result<::http::Response<Vec<u8>>, VmHttpStaticError> {
    let content_length = body.len().to_string();
    let mut builder = ::http::Response::builder()
        .status(status)
        .header(::http::header::CONTENT_LENGTH, content_length);
    if let Some(content_type) = content_type {
        builder = builder.header(::http::header::CONTENT_TYPE, content_type);
    }
    if let Some(cache_control) = cache_control {
        builder = builder.header(::http::header::CACHE_CONTROL, cache_control);
    }
    builder
        .body(body)
        .map_err(|_| VmHttpStaticError::InvalidResponse)
}

fn validate_route_path(route_path: &str) -> Result<(), VmHttpStaticError> {
    if !route_path.starts_with('/') || route_path.contains("..") || route_path.contains('\0') {
        return Err(VmHttpStaticError::InvalidRoute);
    }
    Ok(())
}

fn validate_package_path(package_path: &str) -> Result<(), VmHttpStaticError> {
    if package_path.trim().is_empty() || package_path.contains('\0') {
        return Err(VmHttpStaticError::InvalidAssetPath);
    }
    let path = Path::new(package_path);
    if path.is_absolute() {
        return Err(VmHttpStaticError::InvalidAssetPath);
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(VmHttpStaticError::InvalidAssetPath);
        }
    }
    Ok(())
}
