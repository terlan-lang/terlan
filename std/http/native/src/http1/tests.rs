use std::io::{self, Cursor, Read};

use super::*;

fn decode(bytes: &[u8]) -> http::Request<String> {
    try_parse_http1_request_buffer(bytes).unwrap().unwrap().0
}

#[test]
fn maintained_parser_preserves_metadata_version_and_pipeline_boundaries() {
    let first = b"POST /submit?q=a HTTP/1.0\r\nX-Value: one\r\nX-Value: two\r\nContent-Length: 3\r\n\r\none";
    let second = b"GET /next HTTP/1.1\r\n\r\n";
    let bytes = [first.as_slice(), second.as_slice()].concat();
    let (request, consumed) = try_parse_http1_request_buffer(&bytes).unwrap().unwrap();
    assert_eq!(consumed, first.len());
    assert_eq!(request.method(), http::Method::POST);
    assert_eq!(
        request.uri().path_and_query().unwrap().as_str(),
        "/submit?q=a"
    );
    assert_eq!(request.version(), http::Version::HTTP_10);
    assert_eq!(request.headers().get_all("x-value").iter().count(), 2);
    assert_eq!(request.body(), "one");
    assert_eq!(decode(&bytes[consumed..]).uri(), "/next");
}

#[test]
fn every_fragment_boundary_waits_for_the_full_request() {
    let wire = b"POST /utf8 HTTP/1.1\r\nContent-Length: 4\r\n\r\nf\xc3\xb6o";
    for end in 0..wire.len() {
        assert!(
            try_parse_http1_request_buffer(&wire[..end])
                .unwrap()
                .is_none(),
            "{end}"
        );
    }
    assert_eq!(decode(wire).body().as_bytes(), b"f\xc3\xb6o");
    for limit in [1, 2, 3, 16, 1024] {
        let mut reader = Fragmented {
            inner: Cursor::new(wire),
            limit,
        };
        assert_eq!(
            read_http1_request(&mut reader).unwrap().body().as_bytes(),
            b"f\xc3\xb6o"
        );
    }
}

#[test]
fn ambiguous_or_unsupported_framing_is_rejected_on_both_paths() {
    for fields in [
        "Content-Length: 0\r\nContent-Length: 0",
        "Content-Length: 1\r\ncontent-length: 2",
        "Content-Length: 0, 0",
        "Content-Length: +1",
        "Content-Length: -1",
        "Content-Length:",
        "Content-Length: 1x",
        "Content-Length: 999999999999999999999999999999999",
        "Transfer-Encoding: chunked",
        "Transfer-Encoding: identity",
        "Content-Length: 0\r\nTransfer-Encoding: chunked",
        "Transfer-Encoding: chunked\r\nContent-Length: 0",
    ] {
        let wire = format!("POST / HTTP/1.1\r\n{fields}\r\n\r\nx");
        let buffered = try_parse_http1_request_buffer(wire.as_bytes()).unwrap_err();
        let blocking = read_http1_request(&mut wire.as_bytes()).unwrap_err();
        assert_eq!(blocking.kind, RequestReadFailureKind::Malformed, "{fields}");
        assert_eq!(blocking, buffered, "{fields}");
    }
}

#[test]
fn malformed_header_and_body_bytes_are_rejected() {
    for wire in [
        b"GET / HTTP/1.1\r\nbad header\r\n\r\n".as_slice(),
        b"GET / HTTP/1.1\r\nX-Value: \xff\r\n\r\n",
        b"POST / HTTP/1.1\r\nContent-Length: 1\r\n\r\n\xff",
        b"GET / HTTP/1.2\r\n\r\n",
    ] {
        assert!(try_parse_http1_request_buffer(wire).is_err());
        let mut reader = wire;
        assert_eq!(
            read_http1_request(&mut reader).unwrap_err().kind,
            RequestReadFailureKind::Malformed
        );
    }
}

#[test]
fn header_limit_counts_the_head_not_body_bytes_in_the_same_read() {
    let prefix = "POST / HTTP/1.1\r\nContent-Length: 1024\r\nX-Fill: ";
    let fill = "a".repeat(HTTP_HEADER_LIMIT - prefix.len() - 4);
    let head = format!("{prefix}{fill}\r\n\r\n");
    assert_eq!(head.len(), HTTP_HEADER_LIMIT);
    let wire = format!("{head}{}", "b".repeat(1024));
    assert_eq!(decode(wire.as_bytes()).body().len(), 1024);
    assert_eq!(
        read_http1_request(&mut wire.as_bytes())
            .unwrap()
            .body()
            .len(),
        1024
    );
    let mut coalesced = Fragmented {
        inner: Cursor::new(wire.as_bytes()),
        limit: 997,
    };
    assert_eq!(
        read_http1_request(&mut coalesced).unwrap().body().len(),
        1024
    );

    let oversized = format!("{prefix}a{fill}\r\n\r\n");
    assert_eq!(
        try_parse_http1_request_buffer(oversized.as_bytes()).unwrap_err(),
        header_limit()
    );
    assert_eq!(
        read_http1_request(&mut oversized.as_bytes())
            .unwrap_err()
            .kind,
        RequestReadFailureKind::HeaderLimit
    );
    let partial = &oversized.as_bytes()[..oversized.len() - 1];
    assert!(try_parse_http1_request_buffer(partial).unwrap().is_none());
    let incomplete_oversized = [partial, b"x"].concat();
    assert_eq!(
        try_parse_http1_request_buffer(&incomplete_oversized).unwrap_err(),
        header_limit()
    );
}

#[test]
fn body_limit_is_checked_before_waiting_for_bytes() {
    let wire = format!(
        "POST / HTTP/1.1\r\nContent-Length: {HTTP_BODY_LIMIT}\r\n\r\n{}",
        "x".repeat(HTTP_BODY_LIMIT)
    );
    assert_eq!(decode(wire.as_bytes()).body().len(), HTTP_BODY_LIMIT);
    assert_eq!(
        read_http1_request(&mut wire.as_bytes())
            .unwrap()
            .body()
            .len(),
        HTTP_BODY_LIMIT
    );
    let oversized = format!(
        "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        HTTP_BODY_LIMIT + 1
    );
    let blocking = read_http1_request(&mut oversized.as_bytes()).unwrap_err();
    assert_eq!(blocking.kind, RequestReadFailureKind::BodyLimit);
    assert_eq!(
        try_parse_http1_request_buffer(oversized.as_bytes()).unwrap_err(),
        blocking
    );
}

#[test]
fn upstream_header_count_limit_is_not_silently_truncated() {
    let wire = format!("GET / HTTP/1.1\r\n{}\r\n", "X-Value: 1\r\n".repeat(33));
    assert_eq!(
        read_http1_request(&mut wire.as_bytes()).unwrap_err().kind,
        RequestReadFailureKind::Malformed
    );
    assert!(try_parse_http1_request_buffer(wire.as_bytes()).is_err());
}

#[test]
fn close_wins_across_all_connection_fields_and_http10_defaults_closed() {
    for (wire, expected) in [
        ("GET / HTTP/1.1\r\n\r\n", false),
        ("GET / HTTP/1.0\r\n\r\n", true),
        ("GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n", false),
        (
            "GET / HTTP/1.1\r\nConnection: keep-alive\r\nConnection: CLOSE\r\n\r\n",
            true,
        ),
        ("GET / HTTP/1.1\r\nConnection: upgrade, close\r\n\r\n", true),
        ("GET / HTTP/1.1\r\nConnection: disclose\r\n\r\n", false),
    ] {
        assert_eq!(
            request_wants_http1_close(&decode(wire.as_bytes())),
            expected,
            "{wire}"
        );
    }
    let request = http::Request::builder()
        .header(
            http::header::CONNECTION,
            http::HeaderValue::from_bytes(b"\xff").unwrap(),
        )
        .body(())
        .unwrap();
    assert!(request_wants_http1_close(&request));
}

#[test]
fn eof_and_io_failures_retain_typed_outcomes() {
    for wire in [
        b"".as_slice(),
        b"GET / HTTP/1.1\r\n",
        b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\none",
    ] {
        let mut reader = wire;
        let error = read_http1_request(&mut reader).unwrap_err();
        assert_eq!(error.kind, RequestReadFailureKind::ClientClosed);
        assert_eq!(error.message, incomplete_http1_request_error(wire));
        assert_eq!(error.to_string(), error.message);
    }
    for (kind, expected) in [
        (io::ErrorKind::TimedOut, RequestReadFailureKind::Timeout),
        (io::ErrorKind::WouldBlock, RequestReadFailureKind::Timeout),
        (io::ErrorKind::PermissionDenied, RequestReadFailureKind::Io),
    ] {
        assert_eq!(
            read_http1_request(&mut Failing(kind)).unwrap_err().kind,
            expected
        );
    }
}

#[test]
fn interrupted_reads_are_retried_without_losing_bytes() {
    let mut reader = Interrupted {
        interrupted: false,
        inner: Cursor::new(b"GET / HTTP/1.1\r\n\r\n"),
    };
    assert_eq!(read_http1_request(&mut reader).unwrap().uri(), "/");
}

struct Fragmented<R> {
    inner: R,
    limit: usize,
}
impl<R: Read> Read for Fragmented<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let len = out.len().min(self.limit);
        self.inner.read(&mut out[..len])
    }
}

struct Failing(io::ErrorKind);
impl Read for Failing {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(self.0.into())
    }
}

struct Interrupted<R> {
    interrupted: bool,
    inner: R,
}
impl<R: Read> Read for Interrupted<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.inner.read(out)
    }
}
