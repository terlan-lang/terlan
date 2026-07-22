use super::{VmHttpCompiledHandlerDispatch, VmHttpRouteDispatch, VmHttpRouteTarget};
use crate::runtime::vm::http_static::{
    VmHttp1ResponseStream, VmHttpResponseBody, VmHttpStaticAssetTable, VmHttpStaticError,
};
use crate::runtime::vm::ReplValue;
use crate::terlan_native::http::{self as native_http, CookieOptions, CookieSameSite};

mod stream_descriptor;

/// Materialized response selected by VM router dispatch.
///
/// Inputs: a matched static or explicit response-body route.
/// Output: either a buffered response or a scheduler-owned HTTP/1 stream.
/// Transformation: joins router selection to the existing response machinery
/// without introducing a second transport or bypassing middleware dispatch.
#[derive(Debug)]
pub(crate) enum VmHttpRouteResponse {
    Buffered(::http::Response<Vec<u8>>),
    Streaming(VmHttp1ResponseStream),
}

impl VmHttpCompiledHandlerDispatch {
    /// Converts a compiled Terlan handler result into the router response path.
    ///
    /// Inputs:
    /// - `self`: completed compiled-handler dispatch.
    /// - `assets`: validated manifest assets available to `Response.file`.
    ///
    /// Output:
    /// - A buffered response for the finite `std.http.Response` constructors.
    /// - A typed static error for malformed descriptors, unsafe metadata, or a
    ///   file absent from the manifest.
    ///
    /// Transformation:
    /// - Decodes the VM's compact source-level response descriptor exactly once
    ///   and joins it to the same byte response used by static router targets.
    pub(crate) fn into_std_http_response(
        self,
        assets: &VmHttpStaticAssetTable,
        close_connection: bool,
    ) -> Result<VmHttpRouteResponse, VmHttpStaticError> {
        decode_std_http_response(self.result, assets, close_connection)
    }
}

impl VmHttpRouteDispatch {
    /// Materializes a matched static or explicit body target.
    ///
    /// Inputs:
    /// - `self`: dispatch returned after router and middleware selection.
    /// - `status`: validated response status.
    /// - `close_connection`: HTTP/1 connection policy for streamed output.
    ///
    /// Output:
    /// - Buffered bytes for finite bodies.
    /// - The existing VM HTTP/1 stream state machine for stream bodies.
    /// - `InvalidResponse` when the target still requires handler or channel
    ///   execution before it can produce a response.
    ///
    /// Transformation:
    /// - Preserves one response representation from route dispatch through VM
    ///   TCP framing, including the stream's bounded backpressure contract.
    pub(crate) fn into_http_response(
        self,
        status: ::http::StatusCode,
        close_connection: bool,
    ) -> Result<VmHttpRouteResponse, VmHttpStaticError> {
        match self.target {
            VmHttpRouteTarget::StaticAsset(asset) => VmHttpResponseBody::StaticAsset(asset)
                .into_http_response(status)
                .map(VmHttpRouteResponse::Buffered),
            VmHttpRouteTarget::ResponseBody(VmHttpResponseBody::Stream(plan)) => {
                let response = ::http::Response::builder()
                    .status(status)
                    .body(())
                    .map_err(|_| VmHttpStaticError::InvalidResponse)?;
                plan.open_http1_stream(response, close_connection)
                    .map(VmHttpRouteResponse::Streaming)
            }
            VmHttpRouteTarget::ResponseBody(body) => body
                .into_http_response(status)
                .map(VmHttpRouteResponse::Buffered),
            VmHttpRouteTarget::Handler(_)
            | VmHttpRouteTarget::CompiledHandler(_)
            | VmHttpRouteTarget::SseEndpoint(_)
            | VmHttpRouteTarget::WebSocketEndpoint(_) => Err(VmHttpStaticError::InvalidResponse),
        }
    }
}

/// Decodes one closed `std.http.Response` value into VM transport state.
pub(super) fn decode_std_http_response(
    value: ReplValue,
    assets: &VmHttpStaticAssetTable,
    close_connection: bool,
) -> Result<VmHttpRouteResponse, VmHttpStaticError> {
    let ReplValue::Tuple(fields) = value else {
        return Err(VmHttpStaticError::InvalidResponse);
    };
    let [ReplValue::Atom(tag), ReplValue::Atom(kind), payload, remaining @ ..] = fields.as_slice()
    else {
        return Err(VmHttpStaticError::InvalidResponse);
    };
    if tag != "response" {
        return Err(VmHttpStaticError::InvalidResponse);
    }

    let mut decoded = decode_response_payload(kind, payload, remaining, assets)?;
    apply_response_metadata(&mut decoded, remaining)?;
    decoded.build(close_connection)
}

/// Intermediate response metadata validated before `http` value construction.
struct DecodedStdHttpResponse {
    status: i64,
    body: Vec<u8>,
    stream: Option<stream_descriptor::DecodedStdHttpStream>,
    content_type: Option<String>,
    cache_control: Option<String>,
    location: Option<String>,
    headers: Vec<(::http::HeaderName, ::http::HeaderValue)>,
}

impl DecodedStdHttpResponse {
    /// Builds a buffered or streaming VM response after metadata validation.
    fn build(self, close_connection: bool) -> Result<VmHttpRouteResponse, VmHttpStaticError> {
        let status = u16::try_from(self.status)
            .ok()
            .and_then(|status| ::http::StatusCode::from_u16(status).ok())
            .ok_or(VmHttpStaticError::InvalidResponse)?;
        let mut builder = ::http::Response::builder().status(status);
        if let Some(content_type) = self.content_type {
            builder = builder.header(::http::header::CONTENT_TYPE, content_type);
        }
        if let Some(cache_control) = self.cache_control {
            builder = builder.header(::http::header::CACHE_CONTROL, cache_control);
        }
        if let Some(location) = self.location {
            builder = builder.header(::http::header::LOCATION, location);
        }
        if let Some(stream) = self.stream {
            let mut response = builder
                .body(())
                .map_err(|_| VmHttpStaticError::InvalidResponse)?;
            append_decoded_headers(response.headers_mut(), self.headers);
            let mut output = stream.plan.open_http1_stream(response, close_connection)?;
            for chunk in stream.chunks {
                output.enqueue(chunk)?;
            }
            output.finish()?;
            return Ok(VmHttpRouteResponse::Streaming(output));
        }
        builder = builder.header(::http::header::CONTENT_LENGTH, self.body.len().to_string());
        let mut response = builder
            .body(self.body)
            .map_err(|_| VmHttpStaticError::InvalidResponse)?;
        append_decoded_headers(response.headers_mut(), self.headers);
        Ok(VmHttpRouteResponse::Buffered(response))
    }
}

/// Appends validated repeated headers to the final response map.
fn append_decoded_headers(
    headers: &mut ::http::HeaderMap,
    decoded: Vec<(::http::HeaderName, ::http::HeaderValue)>,
) {
    for (name, value) in decoded {
        headers.append(name, value);
    }
}

/// Decodes the response-kind payload and positional arguments.
fn decode_response_payload(
    kind: &str,
    payload: &ReplValue,
    remaining: &[ReplValue],
    assets: &VmHttpStaticAssetTable,
) -> Result<DecodedStdHttpResponse, VmHttpStaticError> {
    validate_positional_values(kind, remaining)?;
    let status = positional_status(kind, remaining).unwrap_or_else(|| default_status(kind));
    if kind == "stream" {
        return stream_descriptor::decode(payload, remaining, status);
    }
    let response = match (kind, payload) {
        ("text", ReplValue::String(body)) => decoded_body(
            status,
            body.as_bytes().to_vec(),
            "text/plain; charset=utf-8",
        ),
        ("html", ReplValue::String(body)) => {
            decoded_body(status, body.as_bytes().to_vec(), "text/html; charset=utf-8")
        }
        ("html", ReplValue::Tuple(fragment)) => {
            let [ReplValue::Atom(tag), ReplValue::String(body)] = fragment.as_slice() else {
                return Err(VmHttpStaticError::InvalidResponse);
            };
            if tag != "html" {
                return Err(VmHttpStaticError::InvalidResponse);
            }
            decoded_body(status, body.as_bytes().to_vec(), "text/html; charset=utf-8")
        }
        ("json_text", ReplValue::String(body)) => decoded_body(
            status,
            body.as_bytes().to_vec(),
            "application/json; charset=utf-8",
        ),
        ("file", ReplValue::String(package_path)) => {
            let asset = assets.lookup_package_path(package_path)?;
            DecodedStdHttpResponse {
                status,
                body: asset.bytes().to_vec(),
                stream: None,
                content_type: positional_content_type(remaining)
                    .or_else(|| Some(asset.content_type().to_string())),
                cache_control: Some(asset.cache_control().to_string()),
                location: None,
                headers: Vec::new(),
            }
        }
        ("redirect", ReplValue::String(location)) => DecodedStdHttpResponse {
            status,
            body: Vec::new(),
            stream: None,
            content_type: None,
            cache_control: None,
            location: Some(location.clone()),
            headers: Vec::new(),
        },
        _ => return Err(VmHttpStaticError::InvalidResponse),
    };
    Ok(response)
}

/// Validates positional descriptor shapes before metadata tuples begin.
fn validate_positional_values(
    kind: &str,
    remaining: &[ReplValue],
) -> Result<(), VmHttpStaticError> {
    let positional_len = remaining
        .iter()
        .position(|value| matches!(value, ReplValue::Tuple(_)))
        .unwrap_or(remaining.len());
    let positional = &remaining[..positional_len];
    let valid = match kind {
        "text" | "html" | "json" | "json_text" | "redirect" => {
            positional.is_empty() || matches!(positional, [ReplValue::Int(_)])
        }
        "file" => matches!(
            positional,
            [] | [ReplValue::Int(_)]
                | [ReplValue::String(_)]
                | [ReplValue::Int(_), ReplValue::String(_)]
                | [ReplValue::String(_), ReplValue::Int(_)]
        ),
        "stream" => matches!(
            positional,
            [
                ReplValue::Int(_),
                ReplValue::String(_),
                ReplValue::Int(_),
                ReplValue::Int(_)
            ]
        ),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(VmHttpStaticError::InvalidResponse)
    }
}

/// Constructs an intermediate finite response with canonical content type.
fn decoded_body(status: i64, body: Vec<u8>, content_type: &str) -> DecodedStdHttpResponse {
    DecodedStdHttpResponse {
        status,
        body,
        stream: None,
        content_type: Some(content_type.to_string()),
        cache_control: None,
        location: None,
        headers: Vec::new(),
    }
}

/// Extracts an optional positional HTTP status for supported response kinds.
fn positional_status(kind: &str, remaining: &[ReplValue]) -> Option<i64> {
    match kind {
        "text" | "html" | "json" | "json_text" | "file" | "stream" | "redirect" => remaining
            .iter()
            .take_while(|value| !matches!(value, ReplValue::Tuple(_)))
            .find_map(|value| match value {
                ReplValue::Int(status) => Some(*status),
                _ => None,
            }),
        _ => None,
    }
}

/// Extracts an optional nonempty positional content type.
fn positional_content_type(remaining: &[ReplValue]) -> Option<String> {
    remaining
        .iter()
        .take_while(|value| !matches!(value, ReplValue::Tuple(_)))
        .find_map(|value| match value {
            ReplValue::String(content_type) if !content_type.trim().is_empty() => {
                Some(content_type.clone())
            }
            _ => None,
        })
}

/// Returns the default success or redirect status for a response kind.
fn default_status(kind: &str) -> i64 {
    if kind == "redirect" {
        302
    } else {
        200
    }
}

/// Applies validated status, header, and cookie metadata tuples in order.
fn apply_response_metadata(
    response: &mut DecodedStdHttpResponse,
    remaining: &[ReplValue],
) -> Result<(), VmHttpStaticError> {
    for value in remaining
        .iter()
        .skip_while(|value| !matches!(value, ReplValue::Tuple(_)))
    {
        let ReplValue::Tuple(metadata) = value else {
            return Err(VmHttpStaticError::InvalidResponse);
        };
        match metadata.as_slice() {
            [ReplValue::Atom(tag), ReplValue::Int(status)] if tag == "status" => {
                response.status = *status;
            }
            [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(value)]
                if tag == "header" =>
            {
                append_header(response, name, value)?;
            }
            [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(value), ReplValue::String(path), ReplValue::Bool(http_only), ReplValue::Bool(secure)]
                if tag == "cookie" =>
            {
                let value = native_http::set_header(name, value, path, *http_only, *secure)
                    .map_err(|_| VmHttpStaticError::InvalidResponse)?;
                append_set_cookie(response, value)?;
            }
            [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(path)]
                if tag == "delete_cookie" =>
            {
                let value = native_http::delete_header(name, path)
                    .map_err(|_| VmHttpStaticError::InvalidResponse)?;
                append_set_cookie(response, value)?;
            }
            [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(value), ReplValue::String(path), ReplValue::String(domain), ReplValue::Int(max_age), ReplValue::Bool(include_max_age), ReplValue::String(expires), ReplValue::Bool(http_only), ReplValue::Bool(secure), ReplValue::String(same_site)]
                if tag == "cookie_options" =>
            {
                let options = CookieOptions {
                    path: path.clone(),
                    domain: nonempty(domain),
                    max_age: include_max_age.then_some(*max_age),
                    expires: nonempty(expires),
                    http_only: *http_only,
                    secure: *secure,
                    same_site: parse_same_site(same_site)?,
                };
                let value = native_http::set_header_with_options(name, value, &options)
                    .map_err(|_| VmHttpStaticError::InvalidResponse)?;
                append_set_cookie(response, value)?;
            }
            _ => return Err(VmHttpStaticError::InvalidResponse),
        }
    }
    Ok(())
}

/// Validates and appends one repeated Set-Cookie header.
fn append_set_cookie(
    response: &mut DecodedStdHttpResponse,
    value: String,
) -> Result<(), VmHttpStaticError> {
    let value =
        ::http::HeaderValue::from_str(&value).map_err(|_| VmHttpStaticError::InvalidResponse)?;
    response.headers.push((::http::header::SET_COOKIE, value));
    Ok(())
}

/// Converts an empty optional metadata field to `None`.
fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// Decodes the closed SameSite descriptor value domain.
fn parse_same_site(value: &str) -> Result<Option<CookieSameSite>, VmHttpStaticError> {
    match value {
        "" => Ok(None),
        "lax" => Ok(Some(CookieSameSite::Lax)),
        "strict" => Ok(Some(CookieSameSite::Strict)),
        "none" => Ok(Some(CookieSameSite::None)),
        _ => Err(VmHttpStaticError::InvalidResponse),
    }
}

/// Validates and appends a caller-owned non-framing response header.
fn append_header(
    response: &mut DecodedStdHttpResponse,
    name: &str,
    value: &str,
) -> Result<(), VmHttpStaticError> {
    let name = ::http::HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| VmHttpStaticError::InvalidResponse)?;
    if name == ::http::header::CONTENT_LENGTH
        || name == ::http::header::TRANSFER_ENCODING
        || name == ::http::header::CONTENT_TYPE
    {
        return Err(VmHttpStaticError::InvalidResponse);
    }
    let value =
        ::http::HeaderValue::from_str(value).map_err(|_| VmHttpStaticError::InvalidResponse)?;
    response.headers.push((name, value));
    Ok(())
}
