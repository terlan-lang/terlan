use super::packet_decode::{VmHttpPacket, VmHttpUri};
use super::{decode_packet, VmDecodedPacket, VmPacketDecodeOutcome, VmPacketMode, VmPacketOptions};

fn complete(
    mode: VmPacketMode,
    bytes: &[u8],
    options: VmPacketOptions,
) -> (VmDecodedPacket, usize) {
    match decode_packet(mode, bytes, options) {
        VmPacketDecodeOutcome::Complete { packet, consumed } => (packet, consumed),
        outcome => panic!("expected complete packet, got {outcome:?}"),
    }
}

#[test]
fn decode_packet_suite_fixed_modes_preserve_payload_and_rest_contract() {
    let body = b"hello";
    let rest = b"rest";
    let cases = [
        (VmPacketMode::Length1, [vec![5], body.to_vec()].concat(), 1),
        (
            VmPacketMode::Length2,
            [vec![0, 5], body.to_vec()].concat(),
            2,
        ),
        (
            VmPacketMode::Length4,
            [vec![0, 0, 0, 5], body.to_vec()].concat(),
            4,
        ),
    ];
    for (mode, mut framed, header_len) in cases {
        let packet_len = framed.len();
        framed.extend_from_slice(rest);
        assert_eq!(
            complete(mode, &framed, VmPacketOptions::default()),
            (VmDecodedPacket::Bytes(body.to_vec()), packet_len)
        );
        assert_eq!(&framed[packet_len..], rest);
        assert_eq!(packet_len, header_len + body.len());
    }

    let framed_packets = [
        (VmPacketMode::Asn1, b"\x11\x05hello".as_slice()),
        (VmPacketMode::SunRm, b"\x80\x00\x00\x05hello".as_slice()),
        (
            VmPacketMode::Cdr,
            b"GIOP\x01\x02\x00\x04\x00\x00\x00\x05hello".as_slice(),
        ),
        (VmPacketMode::Tpkt, b"\x03\xff\x00\x09hello".as_slice()),
    ];
    for (mode, framed) in framed_packets {
        let with_rest = [framed, rest].concat();
        assert_eq!(
            complete(mode, &with_rest, VmPacketOptions::default()),
            (VmDecodedPacket::Bytes(framed.to_vec()), framed.len())
        );
    }

    let fcgi = b"\x01\x04\x00\x01\x00\x05\x03\xaahelloXYZ";
    assert_eq!(
        complete(VmPacketMode::FastCgi, fcgi, VmPacketOptions::default()),
        (
            VmDecodedPacket::Bytes(b"\x01\x04\x00\x01\x00\x05\x03\xaahello".to_vec()),
            fcgi.len()
        )
    );
}

#[test]
fn decode_packet_suite_partial_limits_validation_and_line_contract() {
    assert_eq!(
        decode_packet(
            VmPacketMode::Length1,
            b"\x05hell",
            VmPacketOptions::default()
        ),
        VmPacketDecodeOutcome::More { total: Some(6) }
    );
    assert_eq!(
        decode_packet(VmPacketMode::Length2, b"\x00", VmPacketOptions::default()),
        VmPacketDecodeOutcome::More { total: None }
    );
    assert_eq!(
        decode_packet(
            VmPacketMode::Length4,
            b"\xff\xff\xff\xff",
            VmPacketOptions::default()
        ),
        VmPacketDecodeOutcome::Invalid
    );
    assert_eq!(
        decode_packet(
            VmPacketMode::Length1,
            b"\x05hello",
            VmPacketOptions::new(4, 0)
        ),
        VmPacketDecodeOutcome::Invalid
    );

    let lines = b"0123456789012345678\nshort\n01234567890123456789\n";
    let (first, first_len) = complete(VmPacketMode::Line, lines, VmPacketOptions::new(20, 0));
    assert_eq!(first, VmDecodedPacket::Bytes(lines[..20].to_vec()));
    let (second, second_len) = complete(
        VmPacketMode::Line,
        &lines[first_len..],
        VmPacketOptions::new(20, 0),
    );
    assert_eq!(second, VmDecodedPacket::Bytes(b"short\n".to_vec()));
    assert_eq!(
        decode_packet(
            VmPacketMode::Line,
            &lines[first_len + second_len..],
            VmPacketOptions::new(20, 0)
        ),
        VmPacketDecodeOutcome::Invalid
    );
    assert_eq!(
        complete(
            VmPacketMode::Line,
            b"abcdefghijk\n",
            VmPacketOptions::new(0, 7)
        ),
        (VmDecodedPacket::Bytes(b"abcdefg".to_vec()), 7)
    );
}

#[test]
fn decode_packet_suite_http_request_response_header_and_fold_contract() {
    let request = b"POST /invalid/url HTTP/1.1\r\nresidue";
    assert_eq!(
        complete(VmPacketMode::Http, request, VmPacketOptions::default()),
        (
            VmDecodedPacket::Http(VmHttpPacket::Request {
                method: "POST".to_string(),
                uri: VmHttpUri::AbsolutePath("/invalid/url".to_string()),
                version: (1, 1),
            }),
            28,
        )
    );
    assert_eq!(
        complete(
            VmPacketMode::Http,
            b"HTTP/1.0 404 Object Not Found\r\nbody",
            VmPacketOptions::default()
        ),
        (
            VmDecodedPacket::Http(VmHttpPacket::Response {
                version: (1, 0),
                status: 404,
                phrase: "Object Not Found".to_string(),
            }),
            31,
        )
    );
    assert_eq!(
        complete(
            VmPacketMode::Http,
            b"HTTP/1.1 200\r\n",
            VmPacketOptions::default()
        )
        .0,
        VmDecodedPacket::Http(VmHttpPacket::Response {
            version: (1, 1),
            status: 200,
            phrase: String::new(),
        })
    );

    let folded = b"Content-Type: text/plain\r\n continued\r\n\tagain\r\nnext";
    assert_eq!(
        complete(VmPacketMode::HttpHeader, folded, VmPacketOptions::default()),
        (
            VmDecodedPacket::Http(VmHttpPacket::Header {
                known_index: 42,
                canonical_name: "Content-Type".to_string(),
                original_name: "Content-Type".to_string(),
                value: "text/plain\r\n continued\r\n\tagain".to_string(),
            }),
            46,
        )
    );
    let unknown = complete(
        VmPacketMode::HttpHeader,
        b"OTHER-field: value\r\n",
        VmPacketOptions::default(),
    )
    .0;
    assert_eq!(
        unknown,
        VmDecodedPacket::Http(VmHttpPacket::Header {
            known_index: 0,
            canonical_name: "Other-Field".to_string(),
            original_name: "OTHER-field".to_string(),
            value: "value".to_string(),
        })
    );
    assert!(matches!(
        complete(
            VmPacketMode::HttpHeader,
            b"Host\t: invalid\r\nrest",
            VmPacketOptions::default()
        )
        .0,
        VmDecodedPacket::Http(VmHttpPacket::Error(_))
    ));
    assert_eq!(
        complete(
            VmPacketMode::HttpHeader,
            b"\r\nbody",
            VmPacketOptions::default()
        ),
        (VmDecodedPacket::Http(VmHttpPacket::EndOfHeaders), 2)
    );
}

#[test]
fn decode_packet_suite_http_uri_ipv6_and_long_incremental_header_contract() {
    let cases = [
        (
            "GET http://[::1]:4000/echo HTTP/1.1\r\n",
            VmHttpUri::Absolute {
                scheme: "http".to_string(),
                host: "[::1]".to_string(),
                port: Some(4000),
                path: "/echo".to_string(),
            },
        ),
        (
            "GET https://example.com:8042/path?q=1 HTTP/1.1\r\n",
            VmHttpUri::Absolute {
                scheme: "https".to_string(),
                host: "example.com".to_string(),
                port: Some(8042),
                path: "/path?q=1".to_string(),
            },
        ),
        (
            "GET ftp://example/path HTTP/1.1\r\n",
            VmHttpUri::Scheme {
                scheme: "ftp".to_string(),
                remainder: "//example/path".to_string(),
            },
        ),
    ];
    for (wire, expected_uri) in cases {
        let VmDecodedPacket::Http(VmHttpPacket::Request { uri, .. }) = complete(
            VmPacketMode::Http,
            wire.as_bytes(),
            VmPacketOptions::default(),
        )
        .0
        else {
            panic!("expected HTTP request");
        };
        assert_eq!(uri, expected_uri);
    }

    let long_header = [b"Link: /".as_slice(), &vec![b'X'; 8_192], b"\r\nnext"].concat();
    assert_eq!(
        decode_packet(
            VmPacketMode::HttpHeader,
            &long_header[..5_000],
            VmPacketOptions::new(16_384, 3_000)
        ),
        VmPacketDecodeOutcome::More { total: None }
    );
    let VmDecodedPacket::Http(VmHttpPacket::Header { value, .. }) = complete(
        VmPacketMode::HttpHeader,
        &long_header,
        VmPacketOptions::new(16_384, 3_000),
    )
    .0
    else {
        panic!("expected long HTTP header");
    };
    assert_eq!(value.len(), 8_193);
    assert_eq!(
        decode_packet(
            VmPacketMode::HttpHeader,
            b"Link: /YYYYYYYYYYYYYYYYYYYY\r\n",
            VmPacketOptions::new(20, 0)
        ),
        VmPacketDecodeOutcome::Invalid
    );
}

#[test]
fn decode_packet_suite_ssl_tls_and_v2_hello_normalization_contract() {
    let body = b"hello";
    assert_eq!(
        complete(
            VmPacketMode::SslTls,
            b"\x19\x22\x11\x00\x05hellorest",
            VmPacketOptions::default()
        ),
        (
            VmDecodedPacket::SslTls {
                content_type: 25,
                version: (34, 17),
                data: body.to_vec(),
            },
            10,
        )
    );
    assert_eq!(
        complete(
            VmPacketMode::SslTls,
            b"\x80\x08\x01\x22\x11hellorest",
            VmPacketOptions::default()
        ),
        (
            VmDecodedPacket::SslTls {
                content_type: 22,
                version: (34, 17),
                data: b"\x01\x00\x00\x07\x22\x11hello".to_vec(),
            },
            10,
        )
    );
}
