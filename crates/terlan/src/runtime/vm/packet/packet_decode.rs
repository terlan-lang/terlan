use super::{
    fixed_packet_length, TCP_PB_1, TCP_PB_2, TCP_PB_4, TCP_PB_ASN1, TCP_PB_CDR, TCP_PB_FCGI,
    TCP_PB_RM, TCP_PB_SSL_TLS, TCP_PB_TPKT,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmPacketMode {
    Length1,
    Length2,
    Length4,
    Asn1,
    SunRm,
    Cdr,
    FastCgi,
    Tpkt,
    SslTls,
    Line,
    Http,
    HttpHeader,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmPacketOptions {
    pub(crate) packet_size: u32,
    pub(crate) line_length: u32,
}

#[cfg(test)]
impl VmPacketOptions {
    pub(crate) fn new(packet_size: u32, line_length: u32) -> Self {
        Self {
            packet_size,
            line_length,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpUri {
    Asterisk,
    Absolute {
        scheme: String,
        host: String,
        port: Option<u16>,
        path: String,
    },
    Scheme {
        scheme: String,
        remainder: String,
    },
    AbsolutePath(String),
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpPacket {
    Request {
        method: String,
        uri: VmHttpUri,
        version: (u16, u16),
    },
    Response {
        version: (u16, u16),
        status: u16,
        phrase: String,
    },
    Header {
        known_index: usize,
        canonical_name: String,
        original_name: String,
        value: String,
    },
    EndOfHeaders,
    Error(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDecodedPacket {
    Bytes(Vec<u8>),
    SslTls {
        content_type: u8,
        version: (u8, u8),
        data: Vec<u8>,
    },
    Http(VmHttpPacket),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmPacketDecodeOutcome {
    Complete {
        packet: VmDecodedPacket,
        consumed: usize,
    },
    More {
        total: Option<usize>,
    },
    Invalid,
}

pub(crate) fn decode_packet(
    mode: VmPacketMode,
    bytes: &[u8],
    options: VmPacketOptions,
) -> VmPacketDecodeOutcome {
    match mode {
        VmPacketMode::Line => decode_line(bytes, options),
        VmPacketMode::Http => decode_http_start(bytes, options),
        VmPacketMode::HttpHeader => decode_http_header(bytes, options),
        _ => decode_fixed(mode, bytes, options),
    }
}

fn decode_fixed(
    mode: VmPacketMode,
    bytes: &[u8],
    options: VmPacketOptions,
) -> VmPacketDecodeOutcome {
    let htype = match mode {
        VmPacketMode::Length1 => TCP_PB_1,
        VmPacketMode::Length2 => TCP_PB_2,
        VmPacketMode::Length4 => TCP_PB_4,
        VmPacketMode::Asn1 => TCP_PB_ASN1,
        VmPacketMode::SunRm => TCP_PB_RM,
        VmPacketMode::Cdr => TCP_PB_CDR,
        VmPacketMode::FastCgi => TCP_PB_FCGI,
        VmPacketMode::Tpkt => TCP_PB_TPKT,
        VmPacketMode::SslTls => TCP_PB_SSL_TLS,
        _ => return VmPacketDecodeOutcome::Invalid,
    };
    let Some(total) = fixed_packet_length(htype, bytes, options.packet_size) else {
        return VmPacketDecodeOutcome::Invalid;
    };
    if total < 0 {
        return VmPacketDecodeOutcome::Invalid;
    }
    if total == 0 {
        return VmPacketDecodeOutcome::More { total: None };
    }
    let total = total as usize;
    if bytes.len() < total {
        return VmPacketDecodeOutcome::More { total: Some(total) };
    }

    match mode {
        VmPacketMode::Length1 => complete_bytes(bytes[1..total].to_vec(), total),
        VmPacketMode::Length2 => complete_bytes(bytes[2..total].to_vec(), total),
        VmPacketMode::Length4 => complete_bytes(bytes[4..total].to_vec(), total),
        VmPacketMode::FastCgi => {
            let padding = usize::from(bytes[6]);
            complete_bytes(bytes[..total - padding].to_vec(), total)
        }
        VmPacketMode::SslTls => decode_ssl_tls(bytes, total),
        _ => complete_bytes(bytes[..total].to_vec(), total),
    }
}

fn decode_ssl_tls(bytes: &[u8], total: usize) -> VmPacketDecodeOutcome {
    if bytes[0] & 0x80 != 0 && bytes[2] == 1 {
        let body = &bytes[5..total];
        let handshake_len = body.len().saturating_add(2);
        let Ok(handshake_len) = u32::try_from(handshake_len) else {
            return VmPacketDecodeOutcome::Invalid;
        };
        if handshake_len > 0x00ff_ffff {
            return VmPacketDecodeOutcome::Invalid;
        }
        let mut data = vec![1, (handshake_len >> 16) as u8, (handshake_len >> 8) as u8];
        data.push(handshake_len as u8);
        data.extend_from_slice(&bytes[3..5]);
        data.extend_from_slice(body);
        return VmPacketDecodeOutcome::Complete {
            packet: VmDecodedPacket::SslTls {
                content_type: 22,
                version: (bytes[3], bytes[4]),
                data,
            },
            consumed: total,
        };
    }
    VmPacketDecodeOutcome::Complete {
        packet: VmDecodedPacket::SslTls {
            content_type: bytes[0],
            version: (bytes[1], bytes[2]),
            data: bytes[5..total].to_vec(),
        },
        consumed: total,
    }
}

fn decode_line(bytes: &[u8], options: VmPacketOptions) -> VmPacketDecodeOutcome {
    let newline_end = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|pos| pos + 1);
    let line_limit = usize::try_from(options.line_length).unwrap_or(usize::MAX);
    let packet_limit = usize::try_from(options.packet_size).unwrap_or(usize::MAX);
    let consumed = match newline_end {
        Some(end) if options.line_length != 0 && end > line_limit => line_limit,
        Some(end) => end,
        None if options.line_length != 0 && bytes.len() >= line_limit => line_limit,
        None if options.packet_size != 0 && bytes.len() >= packet_limit => {
            return VmPacketDecodeOutcome::Invalid;
        }
        None => return VmPacketDecodeOutcome::More { total: None },
    };
    if exceeds_limit(consumed, options.packet_size) {
        return VmPacketDecodeOutcome::Invalid;
    }
    complete_bytes(bytes[..consumed].to_vec(), consumed)
}

fn decode_http_start(bytes: &[u8], options: VmPacketOptions) -> VmPacketDecodeOutcome {
    let Some(line_end) = find_crlf(bytes) else {
        return incomplete_text(bytes, options.packet_size);
    };
    let consumed = line_end + 2;
    if exceeds_limit(consumed, options.packet_size) {
        return VmPacketDecodeOutcome::Invalid;
    }
    let Ok(line) = std::str::from_utf8(&bytes[..line_end]) else {
        return http_error(bytes[..consumed].to_vec(), consumed);
    };
    if let Some(response) = parse_http_response(line) {
        return complete_http(response, consumed);
    }
    if let Some(request) = parse_http_request(line) {
        return complete_http(request, consumed);
    }
    http_error(bytes[..consumed].to_vec(), consumed)
}

fn decode_http_header(bytes: &[u8], options: VmPacketOptions) -> VmPacketDecodeOutcome {
    if bytes.starts_with(b"\r\n") {
        return complete_http(VmHttpPacket::EndOfHeaders, 2);
    }
    let Some(first_end) = find_crlf(bytes) else {
        return incomplete_text(bytes, options.packet_size);
    };
    let mut consumed = first_end + 2;
    while bytes
        .get(consumed)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        let Some(relative_end) = find_crlf(&bytes[consumed..]) else {
            return incomplete_text(bytes, options.packet_size);
        };
        consumed += relative_end + 2;
    }
    if exceeds_limit(consumed, options.packet_size) {
        return VmPacketDecodeOutcome::Invalid;
    }
    let line_bytes = &bytes[..consumed - 2];
    let Ok(line) = std::str::from_utf8(line_bytes) else {
        return http_error(bytes[..consumed].to_vec(), consumed);
    };
    let Some(colon) = line.find(':') else {
        return http_error(bytes[..consumed].to_vec(), consumed);
    };
    let original_name = &line[..colon];
    if original_name.is_empty()
        || original_name
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        return http_error(bytes[..consumed].to_vec(), consumed);
    }
    let value = line[colon + 1..]
        .trim_start_matches([' ', '\t'])
        .to_string();
    let (known_index, canonical_name) = canonical_header(original_name);
    complete_http(
        VmHttpPacket::Header {
            known_index,
            canonical_name,
            original_name: original_name.to_string(),
            value,
        },
        consumed,
    )
}

fn parse_http_request(line: &str) -> Option<VmHttpPacket> {
    let mut parts = line.split(' ');
    let method = parts.next()?;
    let target = parts.next()?;
    let version = parse_http_version(parts.next()?)?;
    if method.is_empty() || target.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(VmHttpPacket::Request {
        method: method.to_string(),
        uri: parse_http_uri(target),
        version,
    })
}

fn parse_http_response(line: &str) -> Option<VmHttpPacket> {
    let mut parts = line.splitn(3, ' ');
    let version = parse_http_version(parts.next()?)?;
    let status = parts.next()?.parse::<u16>().ok()?;
    Some(VmHttpPacket::Response {
        version,
        status,
        phrase: parts.next().unwrap_or_default().to_string(),
    })
}

fn parse_http_version(value: &str) -> Option<(u16, u16)> {
    let version = value.strip_prefix("HTTP/")?;
    let (major, minor) = version.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn parse_http_uri(target: &str) -> VmHttpUri {
    if target == "*" {
        return VmHttpUri::Asterisk;
    }
    if let Some(rest) = target.strip_prefix("http://") {
        return parse_absolute_uri("http", rest);
    }
    if let Some(rest) = target.strip_prefix("https://") {
        return parse_absolute_uri("https", rest);
    }
    if target.starts_with('/') {
        return VmHttpUri::AbsolutePath(target.to_string());
    }
    if let Some((scheme, remainder)) = target.split_once(':') {
        return VmHttpUri::Scheme {
            scheme: scheme.to_string(),
            remainder: remainder.to_string(),
        };
    }
    VmHttpUri::Other(target.to_string())
}

fn parse_absolute_uri(scheme: &str, rest: &str) -> VmHttpUri {
    let (authority, path) = rest
        .find('/')
        .map(|index| (&rest[..index], &rest[index..]))
        .unwrap_or((rest, "/"));
    let (host, port) = if authority.starts_with('[') {
        match authority.find(']') {
            Some(end) => {
                let host = &authority[..=end];
                let port = authority[end + 1..]
                    .strip_prefix(':')
                    .and_then(|port| port.parse::<u16>().ok());
                (host, port)
            }
            None => (authority.split(':').next().unwrap_or(authority), None),
        }
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        match port.parse::<u16>() {
            Ok(port) => (host, Some(port)),
            Err(_) => (authority, None),
        }
    } else {
        (authority, None)
    };
    VmHttpUri::Absolute {
        scheme: scheme.to_string(),
        host: host.to_string(),
        port,
        path: path.to_string(),
    }
}

fn canonical_header(name: &str) -> (usize, String) {
    KNOWN_HEADERS
        .iter()
        .position(|known| known.eq_ignore_ascii_case(name))
        .map(|index| (index + 1, KNOWN_HEADERS[index].to_string()))
        .unwrap_or_else(|| (0, title_case_header(name)))
}

fn title_case_header(name: &str) -> String {
    let mut capitalize = true;
    name.chars()
        .map(|character| {
            if character == '-' {
                capitalize = true;
                return character;
            }
            let mapped = if capitalize {
                character.to_ascii_uppercase()
            } else {
                character.to_ascii_lowercase()
            };
            capitalize = false;
            mapped
        })
        .collect()
}

fn incomplete_text(bytes: &[u8], packet_size: u32) -> VmPacketDecodeOutcome {
    if packet_size != 0 && bytes.len() > packet_size as usize {
        VmPacketDecodeOutcome::Invalid
    } else {
        VmPacketDecodeOutcome::More { total: None }
    }
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|window| window == b"\r\n")
}

fn exceeds_limit(len: usize, limit: u32) -> bool {
    limit != 0 && len > limit as usize
}

fn complete_bytes(packet: Vec<u8>, consumed: usize) -> VmPacketDecodeOutcome {
    VmPacketDecodeOutcome::Complete {
        packet: VmDecodedPacket::Bytes(packet),
        consumed,
    }
}

fn complete_http(packet: VmHttpPacket, consumed: usize) -> VmPacketDecodeOutcome {
    VmPacketDecodeOutcome::Complete {
        packet: VmDecodedPacket::Http(packet),
        consumed,
    }
}

fn http_error(bytes: Vec<u8>, consumed: usize) -> VmPacketDecodeOutcome {
    complete_http(VmHttpPacket::Error(bytes), consumed)
}

const KNOWN_HEADERS: &[&str] = &[
    "Cache-Control",
    "Connection",
    "Date",
    "Pragma",
    "Transfer-Encoding",
    "Upgrade",
    "Via",
    "Accept",
    "Accept-Charset",
    "Accept-Encoding",
    "Accept-Language",
    "Authorization",
    "From",
    "Host",
    "If-Modified-Since",
    "If-Match",
    "If-None-Match",
    "If-Range",
    "If-Unmodified-Since",
    "Max-Forwards",
    "Proxy-Authorization",
    "Range",
    "Referer",
    "User-Agent",
    "Age",
    "Location",
    "Proxy-Authenticate",
    "Public",
    "Retry-After",
    "Server",
    "Vary",
    "Warning",
    "Www-Authenticate",
    "Allow",
    "Content-Base",
    "Content-Encoding",
    "Content-Language",
    "Content-Length",
    "Content-Location",
    "Content-Md5",
    "Content-Range",
    "Content-Type",
    "Etag",
    "Expires",
    "Last-Modified",
    "Accept-Ranges",
    "Set-Cookie",
    "Set-Cookie2",
    "X-Forwarded-For",
    "Cookie",
    "Keep-Alive",
    "Proxy-Connection",
];
