use std::io::{self, Write};

use bytes::Bytes;
use http::{header, HeaderValue, Response};

use super::*;
use crate::HttpResponseChunks;

fn streamed() -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    response.extensions_mut().insert(
        HttpResponseChunks::new(vec![Bytes::from_static(b"abc"), Bytes::new()], 2, 1).unwrap(),
    );
    response
}

#[test]
fn buffered_wire_preserves_binary_payload_and_repeated_headers() {
    let response = Response::builder()
        .status(201)
        .header(header::SET_COOKIE, "a=1")
        .header(header::SET_COOKIE, "b=2")
        .body(vec![0, 255, 10])
        .unwrap();
    let mut wire = Vec::new();
    write_http1_response(&mut wire, &response, true).unwrap();
    assert_eq!(
        wire,
        b"HTTP/1.1 201 Created\r\nContent-Length: 3\r\nConnection: close\r\nset-cookie: a=1\r\nset-cookie: b=2\r\n\r\n\x00\xff\n"
    );
}

#[test]
fn finite_stream_wire_preserves_chunk_boundaries_without_consuming_the_source() {
    let response = streamed();
    let mut first = Vec::new();
    write_http1_response(&mut first, &response, false).unwrap();
    assert_eq!(
        first,
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n2\r\nab\r\n1\r\nc\r\n0\r\n\r\n"
    );
    assert!(!response
        .extensions()
        .get::<HttpResponseChunks>()
        .unwrap()
        .is_complete());
    let mut second = Vec::new();
    write_http1_response(&mut second, &response, false).unwrap();
    assert_eq!(first, second);
}

#[test]
fn stream_primitives_report_exact_byte_counts_and_preserve_binary_chunks() {
    let mut wire = Vec::new();
    let head = write_http1_stream_head(&mut wire, &Response::new(()), true).unwrap();
    assert_eq!(head, wire.len());
    let chunk = write_http1_stream_chunk(&mut wire, &[0, 255]).unwrap();
    assert_eq!(chunk, 7);
    let end = write_http1_stream_end(&mut wire).unwrap();
    assert_eq!(end, 5);
    assert_eq!(wire.len(), head + chunk + end);
    assert_eq!(&wire[head..], b"2\r\n\x00\xff\r\n0\r\n\r\n");
    let before = wire.clone();
    assert_eq!(
        write_http1_stream_chunk(&mut wire, b"").unwrap_err().kind,
        ResponseWriteFailureKind::InvalidMetadata
    );
    assert_eq!(wire, before);
}

#[test]
fn invalid_stream_metadata_is_rejected_before_any_bytes_escape() {
    for status in [100, 101, 199, 204, 304] {
        let mut response = streamed();
        *response.status_mut() = http::StatusCode::from_u16(status).unwrap();
        assert_invalid_without_output(&response);
    }
    for name in [header::CONTENT_LENGTH, header::TRANSFER_ENCODING] {
        let mut response = streamed();
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static("0"));
        assert_invalid_without_output(&response);
    }
}

#[test]
fn invalid_headers_fail_closed_for_both_buffered_and_streamed_responses() {
    for name in [
        header::CONNECTION,
        http::HeaderName::from_static("x-invalid"),
    ] {
        for mut response in [streamed(), Response::new(b"body".to_vec())] {
            response
                .headers_mut()
                .insert(name.clone(), HeaderValue::from_bytes(b"\xff").unwrap());
            assert_invalid_without_output(&response);
        }
    }
    let response = Response::builder()
        .header(
            header::CONTENT_LENGTH,
            HeaderValue::from_bytes(b"\xff").unwrap(),
        )
        .body(Vec::new())
        .unwrap();
    assert_invalid_without_output(&response);
    let response = Response::builder()
        .header(header::TRANSFER_ENCODING, "chunked")
        .body(Vec::new())
        .unwrap();
    assert_invalid_without_output(&response);
}

#[test]
fn explicit_head_response_length_and_connection_policy_are_preserved() {
    let response = Response::builder()
        .header(header::CONTENT_LENGTH, "42")
        .header(header::CONNECTION, "close")
        .body(Vec::new())
        .unwrap();
    let mut wire = Vec::new();
    write_http1_response(&mut wire, &response, false).unwrap();
    assert_eq!(
        wire,
        b"HTTP/1.1 200 OK\r\nContent-Length: 42\r\nConnection: close\r\n\r\n"
    );
}

#[test]
fn failures_are_classified_at_every_buffered_and_streaming_write_stage() {
    for (kind, expected) in [
        (
            io::ErrorKind::BrokenPipe,
            ResponseWriteFailureKind::ClientClosed,
        ),
        (
            io::ErrorKind::ConnectionAborted,
            ResponseWriteFailureKind::ClientClosed,
        ),
        (
            io::ErrorKind::ConnectionReset,
            ResponseWriteFailureKind::ClientClosed,
        ),
        (
            io::ErrorKind::NotConnected,
            ResponseWriteFailureKind::ClientClosed,
        ),
        (
            io::ErrorKind::UnexpectedEof,
            ResponseWriteFailureKind::ClientClosed,
        ),
        (io::ErrorKind::TimedOut, ResponseWriteFailureKind::Timeout),
        (io::ErrorKind::WouldBlock, ResponseWriteFailureKind::Timeout),
        (io::ErrorKind::Other, ResponseWriteFailureKind::Io),
    ] {
        for (response, stages) in [(Response::new(b"body".to_vec()), 2), (streamed(), 4)] {
            for remaining in 0..stages {
                let error =
                    write_http1_response(&mut FailAfter { remaining, kind }, &response, false)
                        .unwrap_err();
                assert_eq!(error.kind, expected, "{kind:?} at write {remaining}");
                assert!(error.message.starts_with("failed to "));
                assert_eq!(error.to_string(), error.message);
            }
        }
    }
}

#[test]
fn partial_writes_and_interruptions_preserve_exact_output() {
    for response in [Response::new(vec![0, 255, 42]), streamed()] {
        let mut expected = Vec::new();
        write_http1_response(&mut expected, &response, false).unwrap();
        let mut writer = PartialWriter {
            bytes: Vec::new(),
            interrupt: true,
        };
        write_http1_response(&mut writer, &response, false).unwrap();
        assert_eq!(writer.bytes, expected);
    }
}

#[test]
fn a_zero_capacity_writer_returns_io_failure_instead_of_spinning() {
    let mut empty = [].as_mut_slice();
    let error = write_http1_response(&mut empty, &Response::new(b"body"), false).unwrap_err();
    assert_eq!(error.kind, ResponseWriteFailureKind::Io);
}

fn assert_invalid_without_output(response: &Response<Vec<u8>>) {
    let mut wire = Vec::new();
    assert_eq!(
        write_http1_response(&mut wire, response, false)
            .unwrap_err()
            .kind,
        ResponseWriteFailureKind::InvalidMetadata
    );
    assert!(wire.is_empty());
}

struct FailAfter {
    remaining: usize,
    kind: io::ErrorKind,
}

impl Write for FailAfter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::from(self.kind));
        }
        self.remaining -= 1;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct PartialWriter {
    bytes: Vec<u8>,
    interrupt: bool,
}

impl Write for PartialWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if std::mem::take(&mut self.interrupt) {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let length = bytes.len().min(3);
        self.bytes.extend_from_slice(&bytes[..length]);
        Ok(length)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
