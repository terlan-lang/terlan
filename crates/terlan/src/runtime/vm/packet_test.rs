use super::{
    fixed_packet_length, TCP_PB_1, TCP_PB_2, TCP_PB_4, TCP_PB_ASN1, TCP_PB_CDR, TCP_PB_FCGI,
    TCP_PB_RAW, TCP_PB_RM, TCP_PB_SSL_TLS, TCP_PB_TPKT,
};

fn parse(htype: i32, bytes: &[u8], max_plen: u32) -> Option<i32> {
    fixed_packet_length(htype, bytes, max_plen)
}

#[test]
fn reports_unsupported_modes_to_caller() {
    assert_eq!(parse(8, b"line\n", 0), None);
    assert_eq!(parse(10, b"GET / HTTP/1.1\r\n", 0), None);
}

#[test]
fn parses_raw_and_simple_prefixes() {
    assert_eq!(parse(TCP_PB_RAW, b"", 0), Some(0));
    assert_eq!(parse(TCP_PB_RAW, b"abc", 0), Some(3));
    assert_eq!(parse(TCP_PB_1, b"\x03abc", 0), Some(4));
    assert_eq!(parse(TCP_PB_2, b"\x00\x03abc", 0), Some(5));
    assert_eq!(parse(TCP_PB_4, b"\x00\x00\x00\x03abc", 0), Some(7));
    assert_eq!(parse(TCP_PB_RM, b"\x80\x00\x00\x03abc", 0), Some(7));
}

#[test]
fn reports_more_for_short_fixed_prefixes() {
    assert_eq!(parse(TCP_PB_1, b"", 0), Some(0));
    assert_eq!(parse(TCP_PB_2, b"\x00", 0), Some(0));
    assert_eq!(parse(TCP_PB_4, b"\x00\x00\x00", 0), Some(0));
}

#[test]
fn enforces_max_packet_length_and_total_overflow() {
    assert_eq!(parse(TCP_PB_1, b"\x03abc", 2), Some(-1));
    assert_eq!(parse(TCP_PB_4, b"\xff\xff\xff\xff", 0), Some(-1));
    assert_eq!(parse(TCP_PB_RM, b"\xff\xff\xff\xff", 0), Some(-1));
}

#[test]
fn parses_asn1_short_and_long_forms() {
    assert_eq!(parse(TCP_PB_ASN1, b"\x11\x03abc", 0), Some(5));
    assert_eq!(
        parse(TCP_PB_ASN1, b"\x1f\x81\x11\x82\x00\x03abc", 0),
        Some(9)
    );
    assert_eq!(parse(TCP_PB_ASN1, b"\x11\x83\x00\x00\x03abc", 0), Some(8));
    assert_eq!(
        parse(TCP_PB_ASN1, b"\x11\x84\x00\x00\x00\x03abc", 0),
        Some(9)
    );
}

#[test]
fn handles_asn1_truncated_and_invalid_lengths() {
    assert_eq!(parse(TCP_PB_ASN1, b"\x11", 0), Some(0));
    assert_eq!(parse(TCP_PB_ASN1, b"\x1f\x81", 0), Some(0));
    assert_eq!(parse(TCP_PB_ASN1, b"\x1f\x81\x11\x82\x00", 0), Some(0));
    assert_eq!(parse(TCP_PB_ASN1, b"\x11\x80", 0), Some(2));
    assert_eq!(parse(TCP_PB_ASN1, b"\x11\x81\x03abc", 0), Some(6));
    assert_eq!(
        parse(TCP_PB_ASN1, b"\x11\x85\x00\x00\x00\x00\x00", 0),
        Some(-1)
    );
}

#[test]
fn parses_cdr_big_and_little_endian() {
    assert_eq!(
        parse(TCP_PB_CDR, b"GIOP\x01\x02\x00\x00\x00\x00\x00\x03abc", 0),
        Some(15)
    );
    assert_eq!(
        parse(TCP_PB_CDR, b"GIOP\x01\x02\x01\x00\x03\x00\x00\x00abc", 0),
        Some(15)
    );
    assert_eq!(
        parse(TCP_PB_CDR, b"NOPE\x01\x02\x00\x00\x00\x00\x00\x03abc", 0),
        Some(-1)
    );
    assert_eq!(parse(TCP_PB_CDR, b"GIOP\x01", 0), Some(0));
}

#[test]
fn parses_fcgi_tpkt_and_ssl_tls() {
    assert_eq!(parse(TCP_PB_FCGI, b"\x01\x02\x00\x01", 0), Some(0));
    assert_eq!(
        parse(TCP_PB_FCGI, b"\x01\x02\x00\x01\x00\x03\x02\x00abcxx", 0),
        Some(13)
    );
    assert_eq!(
        parse(TCP_PB_FCGI, b"\x02\x02\x00\x01\x00\x03\x00\x00abc", 0),
        Some(-1)
    );
    assert_eq!(parse(TCP_PB_TPKT, b"\x03\x00\x00", 0), Some(0));
    assert_eq!(parse(TCP_PB_TPKT, b"\x03\x00\x00\x07abc", 0), Some(7));
    assert_eq!(parse(TCP_PB_TPKT, b"\x04\x00\x00\x07abc", 0), Some(-1));
    assert_eq!(parse(TCP_PB_TPKT, b"\x03\x00\x00\x03", 0), Some(-1));
    assert_eq!(parse(TCP_PB_SSL_TLS, b"\x16\x03\x03\x00", 0), Some(0));
    assert_eq!(
        parse(TCP_PB_SSL_TLS, b"\x16\x03\x03\x00\x03abc", 0),
        Some(8)
    );
    assert_eq!(
        parse(TCP_PB_SSL_TLS, b"\x80\x06\x01\x03\x03abc", 0),
        Some(8)
    );
    assert_eq!(parse(TCP_PB_SSL_TLS, b"\x80\x02\x01\x03\x03", 0), Some(-1));
}

#[test]
fn maximum_payload_boundary_is_consistent_across_framed_modes() {
    let cases = [
        (TCP_PB_1, b"\x03abc".as_slice(), 4),
        (TCP_PB_2, b"\x00\x03abc".as_slice(), 5),
        (TCP_PB_4, b"\x00\x00\x00\x03abc".as_slice(), 7),
        (TCP_PB_RM, b"\x80\x00\x00\x03abc".as_slice(), 7),
        (TCP_PB_ASN1, b"\x11\x03abc".as_slice(), 5),
        (
            TCP_PB_CDR,
            b"GIOP\x01\x02\x00\x00\x00\x00\x00\x03abc".as_slice(),
            15,
        ),
        (
            TCP_PB_FCGI,
            b"\x01\x02\x00\x01\x00\x03\x00\x00abc".as_slice(),
            11,
        ),
        (TCP_PB_TPKT, b"\x03\x00\x00\x07abc".as_slice(), 7),
        (TCP_PB_SSL_TLS, b"\x16\x03\x03\x00\x03abc".as_slice(), 8),
    ];

    for (mode, packet, total) in cases {
        assert_eq!(parse(mode, packet, 3), Some(total));
        assert_eq!(parse(mode, packet, 2), Some(-1));
    }
}
