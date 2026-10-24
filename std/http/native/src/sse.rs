//! Maintained SSE framing only; no Axum server, Tokio runtime, or event scheduling.

use std::{convert::Infallible, time::Duration};

use axum::response::{sse::Event, IntoResponse, Sse};
use futures_util::{stream, FutureExt};
use http_body_util::BodyExt;
use terlan_runtime_abi::{native_binding, BoundaryError, ErrorDomain, NativeBinding};

/// Exact value-only event encoder used by the Terlan response builder.
pub const ENCODE_EVENT: NativeBinding = native_binding!(
    "std.http.sse.encode_event",
    |id: Option<&str>, event: Option<&str>, retry_ms: Option<i64>, data: &str| {
        encode_event(id, event, retry_ms, data)
    }
);

/// Encodes one event with source-normalized data, rejecting metadata injection.
pub fn encode_event(
    id: Option<&str>,
    event: Option<&str>,
    retry_ms: Option<i64>,
    data: &str,
) -> Result<String, BoundaryError> {
    // Axum's builders panic on invalid metadata; validate before invoking them.
    for value in [id, event].into_iter().flatten() {
        if value.contains(['\r', '\n', '\0']) {
            return Err(failure(
                "http.sse.invalid_metadata",
                "event id/name contains CR, LF, or NUL",
            ));
        }
    }
    let mut encoded = Event::default();
    if let Some(id) = id {
        encoded = encoded.id(id);
    }
    if let Some(event) = event {
        encoded = encoded.event(event);
    }
    if let Some(retry_ms) = retry_ms {
        let millis = u64::try_from(retry_ms)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                failure(
                    "http.sse.invalid_retry",
                    "retry must be positive milliseconds",
                )
            })?;
        encoded = encoded.retry(Duration::from_millis(millis));
    }
    if data.contains('\r') {
        return Err(failure(
            "http.sse.unnormalized_data",
            "event data must use LF line endings",
        ));
    }
    encoded = encoded.data(data);
    // A finite iterator is always ready. Poll once; never start an executor or
    // block an actor. Framing is obtained through Axum's public Body interface.
    let body = Sse::new(stream::iter([Ok::<_, Infallible>(encoded)]))
        .into_response()
        .into_body();
    ready_body_text(body)
}

fn ready_body_text(body: axum::body::Body) -> Result<String, BoundaryError> {
    let bytes = body
        .collect()
        .now_or_never()
        .ok_or_else(|| {
            failure(
                "http.sse.codec",
                "in-memory event encoder unexpectedly suspended",
            )
        })?
        .map_err(|error| failure("http.sse.codec", &error.to_string()))?
        .to_bytes();
    String::from_utf8(bytes.to_vec()).map_err(|error| failure("http.sse.codec", &error.to_string()))
}

fn failure(code: &str, message: &str) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeBoundary,
        "SSE encoding",
        format!("error[{code}]: {message}"),
    )
}

#[cfg(test)]
#[path = "sse_test.rs"]
mod tests;
