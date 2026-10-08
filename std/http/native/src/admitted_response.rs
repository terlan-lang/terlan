//! Source response admission and transport projection owned by std.http.

use bytes::Bytes;
use std::borrow::Cow;
use terlan_runtime_abi::DescriptorValue;

use crate::source_descriptor::{response, SourceResponse, SourceResponseBody};
use crate::{build_http_response, HttpError, HttpResponseChunks};

/// An admitted response, independent of the compiler, VM values, and sockets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedResponse {
    pub status: u16,
    pub content_type: Cow<'static, str>,
    pub headers: Vec<(String, String)>,
    pub body: ResponseBody,
}

/// Retains finite payload allocations and pull-driven streaming separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBody {
    Text(String),
    Bytes(Vec<u8>),
    Stream(HttpResponseChunks),
}

impl ResponseBody {
    /// Borrows finite bytes without consuming a stream or assuming it is empty.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Text(body) => Some(body.as_bytes()),
            Self::Bytes(body) => Some(body),
            Self::Stream(_) => None,
        }
    }
}

impl AdmittedResponse {
    /// Validates the full source descriptor before any authorized file access.
    /// The host supplies an authorized reader, called only for a valid file intent.
    pub fn from_source<V: DescriptorValue>(
        value: V,
        read_file: impl FnOnce(&str, String) -> Result<(String, Vec<u8>), HttpError>,
    ) -> Result<Self, HttpError> {
        let SourceResponse {
            status,
            content_type,
            headers,
            body,
        } = response(value)
            .map_err(|error| HttpError::new("http.descriptor", error.message(), 500))?;
        let (content_type, body) = match body {
            SourceResponseBody::Text(payload) => (content_type, ResponseBody::Text(payload)),
            SourceResponseBody::File(path) => {
                let (content_type, bytes) = read_file(&path, content_type)?;
                (content_type, ResponseBody::Bytes(bytes))
            }
            SourceResponseBody::Stream(stream) => (content_type, ResponseBody::Stream(stream)),
        };
        Ok(Self {
            status,
            content_type: Cow::Owned(content_type),
            headers,
            body,
        })
    }

    /// Transfers owned bytes or a pull-driven body to the maintained HTTP codec.
    /// Connection framing belongs to the protocol adapter. HEAD retains finite
    /// content length but never exposes stream work to the transport.
    pub fn into_http(self, head_only: bool) -> Result<http::Response<Bytes>, HttpError> {
        let (body, stream) = match self.body {
            ResponseBody::Text(body) => (Bytes::from(body), None),
            ResponseBody::Bytes(body) => (Bytes::from(body), None),
            ResponseBody::Stream(stream) => (Bytes::new(), Some(stream)),
        };
        let mut response = build_http_response(
            self.status,
            &self.content_type,
            &self.headers,
            body,
            head_only,
            false,
        )?;
        if let Some(stream) = stream {
            response.headers_mut().remove(http::header::CONTENT_LENGTH);
            if !head_only {
                response.extensions_mut().insert(stream);
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
#[path = "admitted_response_test.rs"]
mod tests;
