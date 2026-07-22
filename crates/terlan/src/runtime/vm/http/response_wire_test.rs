use super::test_support::{BodyFailingWriter, ChunkedReader, FailingReader, FailingWriter};
use super::*;

#[test]
fn vm_http_reads_http1_response_with_expected_status() {
    let mut wire = "HTTP/1.1 203 Non-Authoritative Information\r\n\
Content-Length: 2\r\n\
\r\n\
ok"
    .as_bytes();

    let response = read_http1_response(&mut wire, 203).expect("response should parse");

    assert_eq!(
        String::from_utf8(response).expect("response should be UTF-8"),
        "HTTP/1.1 203 Non-Authoritative Information\r\nContent-Length: 2\r\n\r\nok"
    );
}

#[test]
fn vm_http_reads_fragmented_response_body() {
    let mut reader = ChunkedReader::new(vec![
        b"HTTP/1.1 203 OK\r\nContent-Length: 7\r\n\r\npay".to_vec(),
        b"load".to_vec(),
    ]);

    let response = read_http1_response(&mut reader, 203).expect("fragmented response should parse");

    assert_eq!(
        String::from_utf8(response).expect("response should be UTF-8"),
        "HTTP/1.1 203 OK\r\nContent-Length: 7\r\n\r\npayload"
    );
}

#[test]
fn vm_http_rejects_oversized_response_headers() {
    let mut wire = format!("HTTP/1.1 203 OK\r\nX-Fill: {}\r\n", "a".repeat(65 * 1024)).into_bytes();
    let mut reader = wire.as_slice();

    let error = read_http1_response(&mut reader, 203).expect_err("oversized headers should fail");

    assert_eq!(error, "VM HTTP response exceeded 64 KiB header limit");
    wire.clear();
}

#[test]
fn vm_http_rejects_partial_response_headers() {
    let mut wire = "HTTP/1.1 203 OK\r\nContent-Length: 2\r\n".as_bytes();

    let error = read_http1_response(&mut wire, 203).expect_err("partial response should fail");

    assert_eq!(error, "VM HTTP response closed before headers completed");
}

#[test]
fn vm_http_rejects_oversized_response_body_declaration() {
    let mut wire = "HTTP/1.1 203 OK\r\nContent-Length: 1048577\r\n\r\n".as_bytes();

    let error = read_http1_response(&mut wire, 203).expect_err("oversized body should fail");

    assert_eq!(error, "VM HTTP response exceeded 1 MiB body limit");
}

#[test]
fn vm_http_rejects_early_response_body_eof() {
    let mut wire = "HTTP/1.1 203 OK\r\nContent-Length: 8\r\n\r\nshort".as_bytes();

    let error = read_http1_response(&mut wire, 203).expect_err("early body EOF should fail");

    assert_eq!(error, "VM HTTP response body ended early");
}

#[test]
fn vm_http_rejects_invalid_response_content_length() {
    let mut wire = "HTTP/1.1 203 OK\r\nContent-Length: nope\r\n\r\n".as_bytes();

    let error = read_http1_response(&mut wire, 203).expect_err("invalid length should fail");

    assert!(error.contains("VM HTTP response Content-Length `nope` is invalid"));
}

#[test]
fn vm_http_rejects_malformed_response_headers() {
    let mut wire = b"HTTP/1.1 203 OK\r\nbad header\r\n\r\n".as_slice();

    let error = read_http1_response(&mut wire, 203).expect_err("malformed response should fail");

    assert!(error.contains("failed to parse VM HTTP response"));
}

#[test]
fn vm_http_response_header_parser_reports_partial_headers() {
    let error = parse_http1_response_content_length(b"HTTP/1.1 203 OK\r\n", 203)
        .expect_err("partial response parse should fail");

    assert_eq!(error, "VM HTTP response parser reported partial headers");
}

#[test]
fn vm_http_response_content_length_rejects_non_utf8_value() {
    let error = parse_http1_response_content_length(
        b"HTTP/1.1 203 OK\r\nContent-Length: \xff\r\n\r\n",
        203,
    )
    .expect_err("non-UTF-8 length should fail");

    assert!(error.contains("VM HTTP response Content-Length is not UTF-8"));
}

#[test]
fn vm_http_response_content_length_parser_accepts_valid_value() {
    let length =
        parse_http1_response_content_length(b"HTTP/1.1 203 OK\r\nContent-Length: 7\r\n\r\n", 203)
            .expect("content length should parse");

    assert_eq!(length, 7);
}

#[test]
fn vm_http_response_content_length_parser_skips_unrelated_headers() {
    let length = parse_http1_response_content_length(
        b"HTTP/1.1 203 OK\r\nContent-Type: text/plain\r\nContent-Length: 7\r\n\r\n",
        203,
    )
    .expect("content length should parse after unrelated header");

    assert_eq!(length, 7);
}

#[test]
fn vm_http_reports_response_read_error() {
    let mut reader = FailingReader::new("response read failed");

    let error = read_http1_response(&mut reader, 203).expect_err("read error should fail");

    assert!(error.contains("failed to read VM HTTP response"));
}

#[test]
fn vm_http_rejects_http1_response_status_mismatch() {
    let mut wire = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".as_bytes();

    let error = read_http1_response(&mut wire, 203).expect_err("status mismatch should fail");

    assert_eq!(
        error,
        "VM HTTP wire response returned unexpected status: Some(500)"
    );
}

#[test]
fn vm_http_rejects_http1_response_without_content_length() {
    let mut wire = "HTTP/1.1 203 Non-Authoritative Information\r\n\r\n".as_bytes();

    let error = read_http1_response(&mut wire, 203).expect_err("missing length should fail");

    assert_eq!(error, "VM HTTP response missing Content-Length");
}

#[test]
fn vm_http_rejects_invalid_response_header_value_on_write() {
    let invalid = http::HeaderValue::from_bytes(b"\xff").expect("opaque bytes are allowed");
    let response = http::Response::builder()
        .status(200)
        .header("x-invalid", invalid)
        .body("ok".to_string())
        .expect("response should build");
    let mut wire = Vec::new();

    let error =
        write_http1_response(&mut wire, &response, false).expect_err("invalid header should fail");

    assert!(error.contains("VM HTTP response header `x-invalid` is invalid"));
}

#[test]
fn vm_http_rejects_invalid_connection_response_header_on_write() {
    let invalid = http::HeaderValue::from_bytes(b"\xff").expect("opaque bytes are allowed");
    let response = http::Response::builder()
        .status(200)
        .header(http::header::CONNECTION, invalid)
        .body("ok".to_string())
        .expect("response should build");
    let mut wire = Vec::new();

    let error = write_http1_response(&mut wire, &response, false)
        .expect_err("invalid connection should fail");

    assert!(error.contains("VM HTTP response Connection is not valid text"));
}

#[test]
fn vm_http_rejects_invalid_content_length_response_header_on_write() {
    let invalid = http::HeaderValue::from_bytes(b"\xff").expect("opaque bytes are allowed");
    let response = http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_LENGTH, invalid)
        .body("ok".to_string())
        .expect("response should build");
    let mut wire = Vec::new();

    let error =
        write_http1_response(&mut wire, &response, false).expect_err("invalid length should fail");

    assert!(error.contains("VM HTTP response Content-Length is not valid text"));
}

#[test]
fn vm_http_reports_write_error() {
    let response = http::Response::builder()
        .status(200)
        .body("ok".to_string())
        .expect("response should build");
    let mut writer = FailingWriter::new(0, "status write failed");

    let error = write_http1_response(&mut writer, &response, false).expect_err("write should fail");

    assert!(error.contains("failed to write VM HTTP response head"));
}

#[test]
fn vm_http_reports_body_write_error() {
    let response = http::Response::builder()
        .status(200)
        .body("ok".to_string())
        .expect("response should build");
    let mut writer = BodyFailingWriter::new("body write failed");

    let error = write_http1_response(&mut writer, &response, false).expect_err("write should fail");

    assert!(error.contains("failed to write VM HTTP body"));
}
