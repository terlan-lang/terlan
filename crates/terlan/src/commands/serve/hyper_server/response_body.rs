//! Hyper body polling preserves VM stream bounds and transport backpressure.

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::{Body, Frame, SizeHint};

use crate::runtime::vm::http_response_chunks::VmHttpResponseChunks;

/// A buffered response or a finite VM-owned pull-driven stream.
pub(super) enum ResponseBody {
    Buffered(Full<Bytes>),
    Stream(VmHttpResponseChunks),
}

impl ResponseBody {
    /// Takes ownership of the admitted stream before the response enters Hyper.
    pub(super) fn from_response(mut response: http::Response<Bytes>) -> http::Response<Self> {
        let stream = response.extensions_mut().remove::<VmHttpResponseChunks>();
        response.map(|body| match stream {
            Some(stream) => Self::Stream(stream),
            None => Self::Buffered(Full::new(body)),
        })
    }
}

impl Body for ResponseBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut() {
            Self::Buffered(body) => Pin::new(body).poll_frame(context),
            Self::Stream(stream) => {
                Poll::Ready(stream.next_chunk().map(|chunk| Ok(Frame::data(chunk))))
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Self::Buffered(body) => body.is_end_stream(),
            Self::Stream(stream) => stream.is_complete(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Self::Buffered(body) => body.size_hint(),
            Self::Stream(_) => SizeHint::default(),
        }
    }
}

#[cfg(test)]
#[path = "response_body_test.rs"]
mod tests;
