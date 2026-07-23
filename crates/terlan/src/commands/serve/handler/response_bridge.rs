use std::borrow::Cow;
use std::fs;
use std::path::Path;

use crate::runtime::vm::{ReplValue, VmAotHttpResponse};
use crate::terlan_native::http as native_http;

use super::super::package_relative_path;
use super::types::WebPackageResponseHeader;

/// HTTP response returned by a VM/native-backed handler.
///
/// Inputs:
/// - Produced by the handler runtime boundary after validation.
///
/// Output:
/// - Status, content type, and byte body ready for the local HTTP writer.
///
/// Transformation:
/// - Keeps handler runtime output separate from socket writing so handler
///   execution can be tested without binding a server.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandlerResponse {
    pub(crate) status: u16,
    pub(crate) content_type: Cow<'static, str>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: HandlerBody,
}

/// Preserves text ownership while retaining exact bytes for file responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HandlerBody {
    Text(String),
    Bytes(Vec<u8>),
}

impl HandlerBody {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(body) => body.as_bytes(),
            Self::Bytes(body) => body,
        }
    }

    #[allow(dead_code)] // Retained for adapters that can elide empty response bodies.
    pub(crate) fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl HandlerResponse {
    /// Converts the direct managed response envelope copied before heap release.
    pub(crate) fn from_aot_http_response(response: VmAotHttpResponse) -> Result<Self, String> {
        let status = vm_status_to_u16(response.status)?;
        validate_handler_status(status)?;
        let mut headers = response
            .headers
            .into_iter()
            .map(|(name, value)| validate_response_header_owned(name, value))
            .collect::<Result<Vec<_>, _>>()?;
        let (content_type, body) = match response.kind {
            0 => (
                Cow::Borrowed("text/plain; charset=utf-8"),
                HandlerBody::Text(response.payload),
            ),
            1 => (
                Cow::Borrowed("text/html; charset=utf-8"),
                HandlerBody::Text(response.payload),
            ),
            2 => (
                Cow::Borrowed("application/json; charset=utf-8"),
                HandlerBody::Text(response.payload),
            ),
            3 => {
                headers.push(validate_response_header_owned(
                    "Location".to_string(),
                    response.payload,
                )?);
                (
                    Cow::Borrowed("text/plain; charset=utf-8"),
                    HandlerBody::Text(String::new()),
                )
            }
            other => {
                return Err(format!(
                    "error[serve_handler]: unsupported native Response kind `{other}`"
                ))
            }
        };
        Ok(Self {
            status,
            content_type,
            headers,
            body,
        })
    }

    /// Consumes an immediate AOT response without copying its managed body.
    pub(crate) fn from_owned_vm_response_with_package_root(
        response: ReplValue,
        package_root: &Path,
    ) -> Result<Self, String> {
        match response {
            ReplValue::Tuple(fields)
                if matches!(fields.first(), Some(ReplValue::Int(0)))
                    && matches!(fields.get(1), Some(ReplValue::Int(_))) =>
            {
                Self::from_owned_native_response(fields, package_root)
            }
            response => Self::from_vm_response_inner(&response, Some(package_root)),
        }
    }

    /// Decodes the compiler-owned response layout by transferring allocations.
    fn from_owned_native_response(
        fields: Vec<ReplValue>,
        package_root: &Path,
    ) -> Result<Self, String> {
        let mut fields = fields.into_iter();
        let Some(ReplValue::Int(0)) = fields.next() else {
            unreachable!("owned native response guard checks the layout tag");
        };
        let Some(ReplValue::Int(kind)) = fields.next() else {
            unreachable!("owned native response guard checks the kind");
        };
        let kind = native_response_kind(kind)?;
        let mut rest: Vec<_> = fields.collect();

        // File responses already perform filesystem I/O and require their
        // complete optional-field parser. Keep that uncommon path centralized.
        if kind == "file" {
            let mut borrowed = vec![ReplValue::Int(0), ReplValue::Int(4)];
            borrowed.append(&mut rest);
            return Self::from_vm_response_inner(&ReplValue::Tuple(borrowed), Some(package_root));
        }

        let mut headers = if rest.len() > 3 {
            owned_native_response_headers(rest.remove(3))?
        } else {
            Vec::new()
        };
        let mut rest = rest.into_iter();
        let payload = rest.next();
        let status = owned_native_response_status(kind, rest.next())?;
        validate_handler_status(status)?;

        let (content_type, body, headers) = match (kind, payload) {
            ("text", Some(ReplValue::String(body))) => (
                Cow::Borrowed("text/plain; charset=utf-8"),
                HandlerBody::Text(body),
                headers,
            ),
            ("html", Some(ReplValue::String(body))) => (
                Cow::Borrowed("text/html; charset=utf-8"),
                HandlerBody::Text(body),
                headers,
            ),
            ("html", Some(ReplValue::Tuple(mut fragment))) => match fragment.as_mut_slice() {
                [ReplValue::Atom(tag), ReplValue::String(body)] if tag == "html" => (
                    Cow::Borrowed("text/html; charset=utf-8"),
                    HandlerBody::Text(std::mem::take(body)),
                    headers,
                ),
                _ => {
                    return Err(
                        "error[serve_handler]: Response.html expects Template.Html".to_string()
                    )
                }
            },
            ("json_text", Some(ReplValue::String(body))) => (
                Cow::Borrowed("application/json; charset=utf-8"),
                HandlerBody::Text(body),
                headers,
            ),
            ("redirect", Some(ReplValue::String(location))) => {
                headers.push(validate_response_header_owned(
                    "Location".to_string(),
                    location,
                )?);
                (
                    Cow::Borrowed("text/plain; charset=utf-8"),
                    HandlerBody::Text(String::new()),
                    headers,
                )
            }
            ("text", _) => {
                return Err("error[serve_handler]: Response.text expects String".to_string())
            }
            ("html", _) => {
                return Err("error[serve_handler]: Response.html expects Template.Html".to_string())
            }
            ("json_text", _) => {
                return Err("error[serve_handler]: Response.json_text expects String".to_string())
            }
            ("redirect", _) => {
                return Err("error[serve_handler]: Response.redirect expects String".to_string())
            }
            _ => unreachable!("native response kind was validated"),
        };
        Ok(Self {
            status,
            content_type,
            headers,
            body,
        })
    }

    /// Converts a VM-owned `std.http.Response` descriptor with file context.
    ///
    /// Inputs:
    /// - `response`: compact descriptor produced by Terlan VM source
    ///   evaluation.
    /// - `package_root`: root directory of the generated web package.
    ///
    /// Output:
    /// - Internal handler response accepted by the local HTTP writer.
    /// - Stable serve-handler error when the descriptor is malformed, unsafe,
    ///   or points at a file that cannot be served.
    ///
    /// Transformation:
    /// - Resolves `Response.file` paths through the same package-relative
    ///   safety boundary as static manifest file responses before reading the
    ///   body bytes.
    pub(crate) fn from_vm_response_with_package_root(
        response: &ReplValue,
        package_root: &Path,
    ) -> Result<Self, String> {
        Self::from_vm_response_inner(response, Some(package_root))
    }

    /// Converts one VM response descriptor using optional file-serving context.
    fn from_vm_response_inner(
        response: &ReplValue,
        package_root: Option<&Path>,
    ) -> Result<Self, String> {
        let ReplValue::Tuple(fields) = response else {
            return Err("error[serve_handler]: VM handler did not return Response".to_string());
        };
        let (kind, rest, native) = match fields.as_slice() {
            [ReplValue::Atom(tag), ReplValue::Atom(kind), rest @ ..] if tag == "response" => {
                (kind.as_str(), rest, false)
            }
            [ReplValue::Int(0), ReplValue::Int(kind), rest @ ..] => {
                (native_response_kind(*kind)?, rest, true)
            }
            [ReplValue::Atom(_), ..] | [ReplValue::Int(_), ..] => {
                return Err("error[serve_handler]: VM handler did not return Response".to_string())
            }
            _ => return Err("error[serve_handler]: malformed VM Response descriptor".to_string()),
        };
        let (mut status, content_type, body, mut headers) =
            vm_response_base(kind, rest, package_root)?;
        if native {
            apply_native_response_headers(rest.get(3), &mut headers)?;
        }
        let metadata_start = if native {
            rest.len()
        } else {
            vm_response_base_arity(kind, rest)?
        };
        for item in rest.iter().skip(metadata_start) {
            apply_vm_response_metadata(item, &mut status, &mut headers)?;
        }
        validate_handler_status(status)?;
        Ok(Self {
            status,
            content_type: Cow::Owned(content_type),
            headers,
            body: HandlerBody::Bytes(body),
        })
    }
}

/// Reads the native constructor status while retaining existing diagnostics.
fn owned_native_response_status(kind: &str, status: Option<ReplValue>) -> Result<u16, String> {
    match status {
        Some(ReplValue::Int(status)) => vm_status_to_u16(status),
        Some(ReplValue::Tuple(_)) | None => Ok(if kind == "redirect" { 302 } else { 200 }),
        Some(_) => Err(format!(
            "error[serve_handler]: Response.{kind} status must be Int"
        )),
    }
}

/// Consumes repeated native headers after validating the server boundary.
fn owned_native_response_headers(metadata: ReplValue) -> Result<Vec<(String, String)>, String> {
    let ReplValue::List(entries) = metadata else {
        return Err(
            "error[serve_handler]: native Response headers must be List[Header]".to_string(),
        );
    };
    let mut headers = Vec::with_capacity(entries.len());
    for entry in entries {
        let ReplValue::Tuple(fields) = entry else {
            return Err("error[serve_handler]: malformed native Response header".to_string());
        };
        let mut fields = fields.into_iter();
        let (Some(ReplValue::String(name)), Some(ReplValue::String(value)), None) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err("error[serve_handler]: malformed native Response header".to_string());
        };
        headers.push(validate_response_header_owned(name, value)?);
    }
    Ok(headers)
}

/// Validates an already-owned response header without cloning it again.
fn validate_response_header_owned(name: String, value: String) -> Result<(String, String), String> {
    validate_response_header(&name, &value)?;
    Ok((name, value))
}

/// Applies the persistent repeated-header list carried by a managed response.
fn apply_native_response_headers(
    metadata: Option<&ReplValue>,
    headers: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let Some(metadata) = metadata else {
        return Ok(());
    };
    let ReplValue::List(entries) = metadata else {
        return Err(
            "error[serve_handler]: native Response headers must be List[Header]".to_string(),
        );
    };
    for entry in entries {
        let ReplValue::Tuple(fields) = entry else {
            return Err("error[serve_handler]: malformed native Response header".to_string());
        };
        let [ReplValue::String(name), ReplValue::String(value)] = fields.as_slice() else {
            return Err("error[serve_handler]: malformed native Response header".to_string());
        };
        headers.push(validate_response_header(name, value)?);
    }
    Ok(())
}

/// Resolves one compiler-owned managed response-kind discriminant.
fn native_response_kind(kind: i64) -> Result<&'static str, String> {
    match kind {
        0 => Ok("text"),
        1 => Ok("html"),
        2 => Ok("json_text"),
        3 => Ok("redirect"),
        4 => Ok("file"),
        other => Err(format!(
            "error[serve_handler]: unsupported native Response kind `{other}`"
        )),
    }
}

#[cfg(test)]
#[path = "response_bridge_test.rs"]
mod response_bridge_test;

/// Builds the base HTTP response from a VM response kind and payload.
fn vm_response_base(
    kind: &str,
    rest: &[ReplValue],
    package_root: Option<&Path>,
) -> Result<(u16, String, Vec<u8>, Vec<(String, String)>), String> {
    match kind {
        "text" => {
            let (body, status) = vm_text_body_and_status("Response.text", rest, 200)?;
            Ok((
                status,
                "text/plain; charset=utf-8".to_string(),
                body,
                Vec::new(),
            ))
        }
        "html" => {
            let (body, status) = vm_html_body_and_status(rest, 200)?;
            Ok((
                status,
                "text/html; charset=utf-8".to_string(),
                body,
                Vec::new(),
            ))
        }
        "json_text" | "json" => {
            let (body, status) = vm_text_body_and_status("Response.json_text", rest, 200)?;
            Ok((
                status,
                "application/json; charset=utf-8".to_string(),
                body,
                Vec::new(),
            ))
        }
        "redirect" => {
            let [ReplValue::String(location), tail @ ..] = rest else {
                return Err("error[serve_handler]: Response.redirect expects String".to_string());
            };
            let status = match tail.first() {
                Some(ReplValue::Int(status)) => vm_status_to_u16(*status)?,
                Some(_) => {
                    return Err(
                        "error[serve_handler]: Response.redirect status must be Int".to_string()
                    );
                }
                None => 302,
            };
            Ok((
                status,
                "text/plain; charset=utf-8".to_string(),
                Vec::new(),
                vec![validate_response_header("Location", location)?],
            ))
        }
        "file" => vm_file_response(rest, package_root),
        other => Err(format!(
            "error[serve_handler]: unsupported VM Response kind `{other}`"
        )),
    }
}

/// Returns the number of payload fields owned by one response constructor.
fn vm_response_base_arity(kind: &str, rest: &[ReplValue]) -> Result<usize, String> {
    match kind {
        "text" | "html" | "json_text" | "json" | "redirect" => {
            Ok(if matches!(rest.get(1), Some(ReplValue::Int(_))) {
                2
            } else {
                1
            })
        }
        "file" => Ok(match (rest.get(1), rest.get(2)) {
            (Some(ReplValue::Int(_)), Some(ReplValue::String(_))) => 3,
            (Some(ReplValue::Int(_)), _) => 2,
            (Some(ReplValue::String(_)), _) => 2,
            _ => 1,
        }),
        other => Err(format!(
            "error[serve_handler]: unsupported VM Response kind `{other}`"
        )),
    }
}

/// Converts a VM `Response.file` descriptor into a file-backed response.
fn vm_file_response(
    rest: &[ReplValue],
    package_root: Option<&Path>,
) -> Result<(u16, String, Vec<u8>, Vec<(String, String)>), String> {
    let package_root = package_root.ok_or_else(|| {
        "error[serve_handler]: Response.file requires package file-serving context".to_string()
    })?;
    let [ReplValue::String(relative_path), tail @ ..] = rest else {
        return Err("error[serve_handler]: Response.file expects String path".to_string());
    };
    let (status, content_type_override) = vm_file_status_and_content_type(tail)?;
    let response_path = package_relative_path(package_root, relative_path).ok_or_else(|| {
        format!(
            "error[serve_handler]: Response.file path `{relative_path}` is not package-relative"
        )
    })?;
    if !response_path.is_file() {
        return Err(format!(
            "error[serve_handler]: Response.file path `{relative_path}` does not name a file"
        ));
    }
    let body = fs::read(&response_path).map_err(|err| {
        format!("error[serve_handler]: Response.file path `{relative_path}` cannot be read: {err}")
    })?;
    let content_type = content_type_override
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| native_http::content_type_for_path(&response_path));
    Ok((status, content_type, body, Vec::new()))
}

/// Reads optional `Response.file` status and content-type constructor fields.
fn vm_file_status_and_content_type(tail: &[ReplValue]) -> Result<(u16, Option<String>), String> {
    match tail {
        [] | [ReplValue::Tuple(_), ..] => Ok((200, None)),
        [ReplValue::Int(status), ReplValue::String(content_type), ..] => {
            Ok((vm_status_to_u16(*status)?, Some(content_type.clone())))
        }
        [ReplValue::Int(status), ..] => Ok((vm_status_to_u16(*status)?, None)),
        [ReplValue::String(content_type), ..] => Ok((200, Some(content_type.clone()))),
        [other, ..] => Err(format!(
            "error[serve_handler]: Response.file status/content_type has unsupported VM value `{other:?}`"
        )),
    }
}

/// Reads a trusted HTML fragment and optional constructor status.
fn vm_html_body_and_status(
    rest: &[ReplValue],
    default_status: u16,
) -> Result<(Vec<u8>, u16), String> {
    let (body, tail) = match rest {
        [ReplValue::String(body), tail @ ..] => (body.as_str(), tail),
        [ReplValue::Tuple(fragment), tail @ ..] => match fragment.as_slice() {
            [ReplValue::Atom(tag), ReplValue::String(body)] if tag == "html" => {
                (body.as_str(), tail)
            }
            _ => {
                return Err("error[serve_handler]: Response.html expects Template.Html".to_string())
            }
        },
        _ => return Err("error[serve_handler]: Response.html expects Template.Html".to_string()),
    };
    let status = match tail.first() {
        Some(ReplValue::Int(status)) => vm_status_to_u16(*status)?,
        Some(ReplValue::Tuple(_)) | None => default_status,
        Some(_) => return Err("error[serve_handler]: Response.html status must be Int".to_string()),
    };
    Ok((body.as_bytes().to_vec(), status))
}

/// Reads a string body and optional constructor status from a VM descriptor.
fn vm_text_body_and_status(
    name: &str,
    rest: &[ReplValue],
    default_status: u16,
) -> Result<(Vec<u8>, u16), String> {
    let [ReplValue::String(body), tail @ ..] = rest else {
        return Err(format!("error[serve_handler]: {name} expects String"));
    };
    let status = match tail.first() {
        Some(ReplValue::Int(status)) => vm_status_to_u16(*status)?,
        Some(ReplValue::Tuple(_)) | None => default_status,
        Some(_) => return Err(format!("error[serve_handler]: {name} status must be Int")),
    };
    Ok((body.as_bytes().to_vec(), status))
}

/// Applies one VM response metadata item.
fn apply_vm_response_metadata(
    item: &ReplValue,
    status: &mut u16,
    headers: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let ReplValue::Tuple(fields) = item else {
        return Err("error[serve_handler]: malformed VM Response metadata".to_string());
    };
    match fields.as_slice() {
        [ReplValue::Atom(tag), ReplValue::Int(code)] if tag == "status" => {
            *status = vm_status_to_u16(*code)?;
            Ok(())
        }
        [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(value)]
            if tag == "header" =>
        {
            headers.push(validate_response_header(name, value)?);
            Ok(())
        }
        [ReplValue::Atom(tag), ReplValue::String(value)] if tag == "set_cookie" => {
            headers.push(validate_response_header("Set-Cookie", value)?);
            Ok(())
        }
        [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(value), ReplValue::String(path), ReplValue::Bool(http_only), ReplValue::Bool(secure)]
            if tag == "cookie" =>
        {
            let value = native_http::set_header(name, value, path, *http_only, *secure)
                .map_err(vm_cookie_error)?;
            headers.push(validate_response_header("Set-Cookie", &value)?);
            Ok(())
        }
        [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(value), ReplValue::String(path), ReplValue::String(domain), ReplValue::Int(max_age), ReplValue::Bool(include_max_age), ReplValue::String(expires), ReplValue::Bool(http_only), ReplValue::Bool(secure), ReplValue::String(same_site)]
            if tag == "cookie_options" =>
        {
            let options = vm_cookie_options(
                path,
                domain,
                *max_age,
                *include_max_age,
                expires,
                *http_only,
                *secure,
                same_site,
            )?;
            let value = native_http::set_header_with_options(name, value, &options)
                .map_err(vm_cookie_error)?;
            headers.push(validate_response_header("Set-Cookie", &value)?);
            Ok(())
        }
        [ReplValue::Atom(tag), ReplValue::String(name), ReplValue::String(path)]
            if tag == "delete_cookie" =>
        {
            let value = native_http::delete_header(name, path).map_err(vm_cookie_error)?;
            headers.push(validate_response_header("Set-Cookie", &value)?);
            Ok(())
        }
        _ => Err("error[serve_handler]: unsupported VM Response metadata".to_string()),
    }
}

/// Converts VM cookie option metadata into the native cookie serializer shape.
#[allow(clippy::too_many_arguments)]
fn vm_cookie_options(
    path: &str,
    domain: &str,
    max_age: i64,
    include_max_age: bool,
    expires: &str,
    http_only: bool,
    secure: bool,
    same_site: &str,
) -> Result<native_http::CookieOptions, String> {
    let mut options = native_http::CookieOptions::defaults();
    options.path = path.to_string();
    options.domain = non_empty_string(domain);
    options.max_age = include_max_age.then_some(max_age);
    options.expires = non_empty_string(expires);
    options.http_only = http_only;
    options.secure = secure;
    options.same_site = vm_cookie_same_site(same_site)?;
    Ok(options)
}

/// Converts optional VM SameSite text into a native policy marker.
fn vm_cookie_same_site(value: &str) -> Result<Option<native_http::CookieSameSite>, String> {
    match value {
        "" => Ok(None),
        "lax" => Ok(Some(native_http::CookieSameSite::Lax)),
        "strict" => Ok(Some(native_http::CookieSameSite::Strict)),
        "none" => Ok(Some(native_http::CookieSameSite::None)),
        other => Err(format!(
            "error[serve_handler]: unsupported cookie SameSite value `{other}`"
        )),
    }
}

/// Returns an owned optional string when the input is not empty.
fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

/// Converts native cookie serializer errors into serve-handler diagnostics.
fn vm_cookie_error(error: native_http::HttpError) -> String {
    format!(
        "error[serve_handler]: {}: {}",
        error.code(),
        error.message()
    )
}

/// Validates final handler status.
fn validate_handler_status(status: u16) -> Result<(), String> {
    if !(100..=599).contains(&status) {
        return Err(format!(
            "error[serve_handler]: VM HTTP response status `{status}` is outside HTTP range"
        ));
    }
    Ok(())
}

/// Converts VM integer status into the HTTP status range.
fn vm_status_to_u16(status: i64) -> Result<u16, String> {
    if !(100..=599).contains(&status) {
        return Err(format!(
            "error[serve_handler]: VM HTTP response status `{status}` is outside HTTP range"
        ));
    }
    Ok(status as u16)
}

/// Converts static response headers into the HTTP writer tuple shape.
///
/// Inputs:
/// - `headers`: manifest header objects already accepted by package
///   validation.
///
/// Output:
/// - Header name/value tuples used by the local HTTP writer.
///
/// Transformation:
/// - Performs one final validation pass before socket emission so hand-authored
///   manifests still cannot bypass the response-header safety boundary.
pub(crate) fn static_response_header_tuples(
    headers: &[WebPackageResponseHeader],
) -> Result<Vec<(String, String)>, String> {
    headers
        .iter()
        .map(|header| validate_response_header(&header.name, &header.value))
        .collect()
}

/// Validates one response header accepted from a handler boundary.
///
/// Inputs:
/// - `name`: handler-provided header name.
/// - `value`: handler-provided header value.
///
/// Output:
/// - Sanitized owned name/value pair when the header can be emitted.
/// - Stable `error[serve_handler]` diagnostic otherwise.
///
/// Transformation:
/// - Rejects empty names, non-token characters, CR/LF injection, and
///   server-owned headers whose values are produced by the local HTTP writer.
pub(super) fn validate_response_header(
    name: &str,
    value: &str,
) -> Result<(String, String), String> {
    if name.is_empty() || !name.bytes().all(is_http_token_byte) {
        return Err(format!(
            "error[serve_handler]: response header name `{name}` is not a valid HTTP token"
        ));
    }
    if is_server_owned_response_header(name) {
        return Err(format!(
            "error[serve_handler]: response header `{name}` is owned by the server bridge"
        ));
    }
    if value.bytes().any(|byte| byte == b'\r' || byte == b'\n') {
        return Err(format!(
            "error[serve_handler]: response header `{name}` contains a line break"
        ));
    }
    Ok((name.to_string(), value.to_string()))
}

/// Returns whether a byte is allowed inside an HTTP token.
///
/// Inputs:
/// - `byte`: candidate header-name byte.
///
/// Output:
/// - `true` when the byte is accepted by the conservative HTTP token subset.
///
/// Transformation:
/// - Implements the RFC token character set needed for response header names.
fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// Returns whether a response header is controlled by the server bridge.
///
/// Inputs:
/// - `name`: handler-provided header name.
///
/// Output:
/// - `true` when the bridge renders the header itself.
///
/// Transformation:
/// - Keeps handler metadata from conflicting with the local server's required
///   HTTP framing and bridge-owned representation headers.
fn is_server_owned_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "content-type" | "content-length" | "connection" | "cache-control"
    )
}
