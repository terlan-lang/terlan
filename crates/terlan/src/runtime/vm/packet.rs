mod packet_decode;

pub(crate) use packet_decode::{
    decode_packet, VmDecodedPacket, VmPacketDecodeOutcome, VmPacketMode, VmPacketOptions,
};

/// Raw packet mode.
pub(crate) const TCP_PB_RAW: i32 = 0;
/// One-byte length-prefixed packet mode.
pub(crate) const TCP_PB_1: i32 = 1;
/// Two-byte big-endian length-prefixed packet mode.
pub(crate) const TCP_PB_2: i32 = 2;
/// Four-byte big-endian length-prefixed packet mode.
pub(crate) const TCP_PB_4: i32 = 3;
/// ASN.1 BER packet mode.
pub(crate) const TCP_PB_ASN1: i32 = 4;
/// Erlang distribution packet mode.
pub(crate) const TCP_PB_RM: i32 = 5;
/// CORBA CDR packet mode.
pub(crate) const TCP_PB_CDR: i32 = 6;
/// FastCGI packet mode.
pub(crate) const TCP_PB_FCGI: i32 = 7;
/// TPKT packet mode.
pub(crate) const TCP_PB_TPKT: i32 = 9;
/// SSL/TLS packet mode.
pub(crate) const TCP_PB_SSL_TLS: i32 = 12;

const CDR_HEADER_LEN: usize = 12;
const FCGI_HEADER_LEN: usize = 8;
const TPKT_HEADER_LEN: usize = 4;
const SSL_TLS_HEADER_LEN: usize = 5;
const CDR_MAGIC: &[u8; 4] = b"GIOP";
const FCGI_VERSION_1: u8 = 1;
const TPKT_VERSION: u8 = 3;

/// Decodes a fixed-format packet length.
///
/// Returns `None` for unsupported modes, `Some(0)` when more input is needed,
/// `Some(-1)` for malformed or oversized input, and a positive total packet
/// length when the prefix is complete.
pub(crate) fn fixed_packet_length(htype: i32, bytes: &[u8], max_plen: u32) -> Option<i32> {
    match htype {
        TCP_PB_RAW => Some(if bytes.is_empty() {
            0
        } else {
            bytes.len() as i32
        }),
        TCP_PB_1 => fixed_remaining(1, bytes.first().copied().map(u32::from), max_plen),
        TCP_PB_2 => fixed_remaining(2, read_be_u16(bytes).map(u32::from), max_plen),
        TCP_PB_4 => fixed_remaining(4, read_be_u32(bytes), max_plen),
        TCP_PB_RM => fixed_remaining(
            4,
            read_be_u32(bytes).map(|value| value & 0x7fff_ffff),
            max_plen,
        ),
        TCP_PB_ASN1 => asn1_length(bytes, max_plen),
        TCP_PB_CDR => cdr_length(bytes, max_plen),
        TCP_PB_FCGI => fcgi_length(bytes, max_plen),
        TCP_PB_TPKT => tpkt_length(bytes, max_plen),
        TCP_PB_SSL_TLS => ssl_tls_length(bytes, max_plen),
        _ => None,
    }
}

fn fixed_remaining(hlen: usize, plen: Option<u32>, max_plen: u32) -> Option<i32> {
    Some(match plen {
        Some(plen) => remaining(hlen, plen, max_plen),
        None => 0,
    })
}

fn remaining(hlen: usize, plen: u32, max_plen: u32) -> i32 {
    if max_plen != 0 && plen > max_plen {
        return -1;
    }

    let total = hlen as u64 + u64::from(plen);
    if total > i32::MAX as u64 {
        return -1;
    }

    total as i32
}

fn asn1_length(bytes: &[u8], max_plen: u32) -> Option<i32> {
    if bytes.len() < 2 {
        return Some(0);
    }

    let mut pos = 1;
    if bytes[0] & 0x1f == 0x1f {
        while pos < bytes.len() && bytes[pos] & 0x80 == 0x80 {
            pos += 1;
        }
        if bytes.len() - pos < 2 {
            return Some(0);
        }
        pos += 1;
    }

    let length_byte = bytes[pos];
    pos += 1;
    let plen = if length_byte & 0x80 == 0x80 {
        let length_len = usize::from(length_byte & 0x7f);
        if bytes.len() - pos < length_len {
            return Some(0);
        }
        let value = match length_len {
            0 => 0,
            1 => u32::from(bytes[pos]),
            2 => u32::from(read_be_u16(&bytes[pos..]).expect("length already checked")),
            3 => read_be_u24(&bytes[pos..]).expect("length already checked"),
            4 => read_be_u32(&bytes[pos..]).expect("length already checked"),
            _ => return Some(-1),
        };
        pos += length_len;
        value
    } else {
        u32::from(length_byte & 0x7f)
    };

    Some(remaining(pos, plen, max_plen))
}

fn cdr_length(bytes: &[u8], max_plen: u32) -> Option<i32> {
    if bytes.len() < CDR_HEADER_LEN {
        return Some(0);
    }
    if &bytes[0..4] != CDR_MAGIC {
        return Some(-1);
    }

    let plen = if bytes[6] & 0x01 != 0 {
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]])
    } else {
        read_be_u32(&bytes[8..]).expect("length already checked")
    };
    Some(remaining(CDR_HEADER_LEN, plen, max_plen))
}

fn fcgi_length(bytes: &[u8], max_plen: u32) -> Option<i32> {
    if bytes.len() < FCGI_HEADER_LEN {
        return Some(0);
    }
    if bytes[0] != FCGI_VERSION_1 {
        return Some(-1);
    }
    let content_len = u32::from(read_be_u16(&bytes[4..]).expect("length already checked"));
    let padding_len = u32::from(bytes[6]);
    Some(remaining(
        FCGI_HEADER_LEN,
        content_len + padding_len,
        max_plen,
    ))
}

fn tpkt_length(bytes: &[u8], max_plen: u32) -> Option<i32> {
    if bytes.len() < TPKT_HEADER_LEN {
        return Some(0);
    }
    if bytes[0] != TPKT_VERSION {
        return Some(-1);
    }
    let total = u32::from(read_be_u16(&bytes[2..]).expect("length already checked"));
    if total < TPKT_HEADER_LEN as u32 {
        return Some(-1);
    }
    Some(remaining(
        TPKT_HEADER_LEN,
        total - TPKT_HEADER_LEN as u32,
        max_plen,
    ))
}

fn ssl_tls_length(bytes: &[u8], max_plen: u32) -> Option<i32> {
    if bytes.len() < SSL_TLS_HEADER_LEN {
        return Some(0);
    }
    let plen = if bytes[0] & 0x80 != 0 && bytes[2] == 1 {
        let total = u32::from(read_be_u16(bytes).expect("length already checked") & 0x7fff);
        if total < 3 {
            return Some(-1);
        }
        total - 3
    } else {
        u32::from(read_be_u16(&bytes[3..]).expect("length already checked"))
    };
    Some(remaining(SSL_TLS_HEADER_LEN, plen, max_plen))
}

fn read_be_u16(bytes: &[u8]) -> Option<u16> {
    bytes
        .get(..2)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_be_u24(bytes: &[u8]) -> Option<u32> {
    bytes
        .get(..3)
        .map(|bytes| (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
}

fn read_be_u32(bytes: &[u8]) -> Option<u32> {
    bytes
        .get(..4)
        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
#[path = "packet_test.rs"]
#[cfg(test)]
mod packet_test;

#[cfg(test)]
#[path = "packet_decode_beam_suite_parity_test.rs"]
#[cfg(test)]
mod packet_decode_beam_suite_parity_test;
