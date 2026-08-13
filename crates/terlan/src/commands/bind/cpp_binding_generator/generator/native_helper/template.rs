/// Package-neutral helper protocol and process-local resource store.
pub(super) const HELPER_TEMPLATE: &str = r##"#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use @CRATE@::ffi;

const MAX_ADAPTER_FRAME_BYTES: usize = @MAX_FRAME_BYTES@;
const MAX_ADAPTER_TRANSFER_BYTES: usize = @MAX_TRANSFER_BYTES@;
// Text framing base64-expands copied buffers. Keep the decoded public transfer
// limit distinct from this finite wire-message allowance.
const MAX_ADAPTER_MESSAGE_BYTES: usize =
    MAX_ADAPTER_TRANSFER_BYTES * 4 / 3 + MAX_ADAPTER_FRAME_BYTES;

fn main() {
    let mut worker = match Worker::new() {
        Ok(worker) => worker,
        Err(error) => {
            eprintln!("native worker identity initialization failed: {error}");
            return;
        }
    };
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut stdout = io::stdout().lock();
    let mut transfer = InboundTransfer::default();
    loop {
        let mut frame = Vec::new();
        let read = Read::by_ref(&mut input)
            .take((MAX_ADAPTER_FRAME_BYTES + 1) as u64)
            .read_until(b'\n', &mut frame);
        let (payload, terminate) = match read {
            Ok(0) => break,
            Ok(_) if frame.len() > MAX_ADAPTER_FRAME_BYTES => (
                protocol_error("frame_too_large", "native adapter frame exceeds its bound"),
                true,
            ),
            Ok(_) => match String::from_utf8(frame) {
                Ok(line) => match transfer.accept(line.trim_end_matches(['\r', '\n'])) {
                    Ok(Some(request)) => (worker.execute_line(&request), false),
                    Ok(None) => continue,
                    Err(error) => (error, true),
                },
                Err(error) => (protocol_error("invalid_utf8", &error.to_string()), false),
            },
            Err(error) => (protocol_error("native_read_error", &error.to_string()), true),
        };
        if write_response(&mut stdout, &payload).is_err() || terminate {
            break;
        }
    }
}

#[derive(Default)]
struct InboundTransfer {
    request_id: Option<u64>,
    next_index: u64,
    bytes: Vec<u8>,
}

impl InboundTransfer {
    fn accept(&mut self, line: &str) -> Result<Option<String>, String> {
        if !line.starts_with("chunk ") {
            if self.request_id.is_some() {
                return Err(protocol_error(
                    "transfer_interleaved",
                    "chunked request was interrupted",
                ));
            }
            return Ok(Some(line.to_string()));
        }
        let mut fields = line.split_whitespace();
        let _chunk = fields.next();
        let request_id = parse_u64_field(fields.next(), "missing chunk request id")?;
        let index = parse_u64_field(fields.next(), "missing chunk index")?;
        let final_chunk = match fields.next() {
            Some("0") => false,
            Some("1") => true,
            _ => {
                return Err(protocol_error(
                    "invalid_transfer_chunk",
                    "invalid final-chunk marker",
                ))
            }
        };
        let encoded = fields.next().ok_or_else(|| {
            protocol_error("invalid_transfer_chunk", "missing encoded chunk")
        })?;
        if fields.next().is_some() {
            return Err(protocol_error(
                "invalid_transfer_chunk",
                "unexpected chunk fields",
            ));
        }
        match self.request_id {
            None if index == 0 => self.request_id = Some(request_id),
            Some(active) if active == request_id && index == self.next_index => {}
            _ => {
                return Err(protocol_error(
                    "invalid_transfer_chunk",
                    "chunk identity or sequence mismatch",
                ))
            }
        }
        let decoded = STANDARD
            .decode(encoded)
            .map_err(|error| protocol_error("invalid_transfer_chunk", &error.to_string()))?;
        if self.bytes.len().saturating_add(decoded.len()) > MAX_ADAPTER_MESSAGE_BYTES {
            return Err(protocol_error(
                "transfer_too_large",
                "native adapter transfer exceeds its bound",
            ));
        }
        self.bytes.extend_from_slice(&decoded);
        self.next_index += 1;
        if !final_chunk {
            return Ok(None);
        }
        let bytes = std::mem::take(&mut self.bytes);
        self.request_id = None;
        self.next_index = 0;
        let request = String::from_utf8(bytes)
            .map_err(|error| protocol_error("invalid_utf8", &error.to_string()))?;
        if request_id_from_call(&request) != Some(request_id) {
            return Err(protocol_error(
                "invalid_transfer_chunk",
                "assembled request identity mismatch",
            ));
        }
        Ok(Some(request))
    }
}

fn parse_u64_field(value: Option<&str>, missing: &str) -> Result<u64, String> {
    value
        .ok_or_else(|| protocol_error("invalid_transfer_chunk", missing))?
        .parse::<u64>()
        .map_err(|error| protocol_error("invalid_transfer_chunk", &error.to_string()))
}

fn write_response(output: &mut impl Write, response: &str) -> io::Result<()> {
    if response.len() < MAX_ADAPTER_FRAME_BYTES {
        writeln!(output, "{response}")?;
        return output.flush();
    }
    let request_id = request_id_from_reply(response).unwrap_or(0);
    if response.len() > MAX_ADAPTER_MESSAGE_BYTES {
        writeln!(
            output,
            "reply {request_id} 1 {}",
            protocol_error(
                "transfer_too_large",
                "native adapter response exceeds its bound"
            )
        )?;
        return output.flush();
    }
    // Base64 expands by 4/3; this decoded chunk size leaves ample room for the
    // reply header while guaranteeing every physical frame remains bounded.
    let chunk_bytes = (MAX_ADAPTER_FRAME_BYTES - 128) * 3 / 4;
    let chunks = response.as_bytes().chunks(chunk_bytes);
    let chunk_count = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let final_chunk = usize::from(index + 1 == chunk_count);
        writeln!(
            output,
            "reply_chunk {request_id} {index} {final_chunk} {}",
            STANDARD.encode(chunk)
        )?;
    }
    output.flush()
}

fn request_id_from_call(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace();
    (fields.next() == Some("call"))
        .then(|| fields.next()?.parse::<u64>().ok())
        .flatten()
}

fn request_id_from_reply(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace();
    (fields.next() == Some("reply"))
        .then(|| fields.next()?.parse::<u64>().ok())
        .flatten()
}

struct Worker {
    owner: String,
    last_request_id: Option<u64>,
    next_id: u64,
    handles: HashMap<u64, HandleEntry>,
}

struct HandleEntry {
    generation: u64,
    type_name: &'static str,
    value: HandleValue,
}

enum HandleValue {
@HANDLE_VARIANTS@}

#[derive(Clone)]
struct HandleArg {
    owner: String,
    id: u64,
    generation: u64,
    type_name: String,
}

/// One decoded copied record supplied by the Terlan VM.
struct RecordArg {
    /// Public generated record type name.
    name: String,
    /// Primitive fields indexed by their public generated names.
    fields: HashMap<String, RecordField>,
}

/// One primitive copied-record field admitted by the generated boundary.
// The protocol parser accepts every primitive field shape even when one package
// maps only a subset into public operations.
#[allow(
    dead_code,
    reason = "the generated wire decoder stays protocol-complete across package-specific operation subsets"
)]
enum RecordField {
    /// Signed integer field.
    Int(i64),
    /// Double-precision field.
    Float(f64),
    /// Boolean field.
    Bool(bool),
}
#[allow(
    dead_code,
    reason = "copied-record accessors stay protocol-complete across package-specific field subsets"
)]
impl RecordArg {
    /// Resolves one field after checking the public record identity.
    fn field(&self, expected_name: &str, field: &str) -> Result<&RecordField, String> {
        if self.name != expected_name {
            return Err(protocol_error("record_type_mismatch", &self.name));
        }
        self.fields
            .get(field)
            .ok_or_else(|| protocol_error("missing_record_field", field))
    }

    /// Copies one signed integer field.
    fn int(&self, expected_name: &str, field: &str) -> Result<i64, String> {
        match self.field(expected_name, field)? {
            RecordField::Int(value) => Ok(*value),
            _ => Err(protocol_error("record_field_type_mismatch", field)),
        }
    }

    /// Copies one double-precision field.
    fn float(&self, expected_name: &str, field: &str) -> Result<f64, String> {
        match self.field(expected_name, field)? {
            RecordField::Float(value) => Ok(*value),
            _ => Err(protocol_error("record_field_type_mismatch", field)),
        }
    }

    /// Copies one Boolean field.
    fn bool(&self, expected_name: &str, field: &str) -> Result<bool, String> {
        match self.field(expected_name, field)? {
            RecordField::Bool(value) => Ok(*value),
            _ => Err(protocol_error("record_field_type_mismatch", field)),
        }
    }
}

// Decoding remains protocol-complete while generated operation arms consume only
// the argument variants declared by this package.
#[allow(
    dead_code,
    reason = "the generated wire decoder stays protocol-complete across package-specific operation subsets"
)]
enum Arg {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    EmptyList,
    Atom(String),
    Record(RecordArg),
    Handle(HandleArg),
}

struct Request {
    operation: String,
    args: Vec<Arg>,
}

impl Worker {
    fn new() -> Result<Self, getrandom::Error> {
        let mut owner = [0_u8; 32];
        getrandom::fill(&mut owner)?;
        Ok(Self {
            owner: STANDARD.encode(owner),
            last_request_id: None,
            next_id: 0,
            handles: HashMap::new(),
        })
    }

    fn execute_line(&mut self, line: &str) -> String {
        let Some(request_id) = request_id(line) else {
            return match parse_request(line) {
                Ok(_) => protocol_error("invalid_request", "request id is missing"),
                Err(error) => error,
            };
        };
        if self
            .last_request_id
            .is_some_and(|last_request_id| request_id <= last_request_id)
        {
            return format!(
                "reply {request_id} 1 {}",
                protocol_error(
                    "request_not_monotonic",
                    "native adapter request id was already completed"
                )
            );
        }
        self.last_request_id = Some(request_id);
        let request = match parse_request(line) {
            Ok(request) => request,
            Err(error) => return format!("reply {request_id} 1 {error}"),
        };
        let payload = self.execute(request);
        format!("reply {request_id} 1 {payload}")
    }

    fn execute(&mut self, request: Request) -> String {
        match request.operation.as_str() {
@OPERATION_ARMS@            _ => protocol_error("unknown_operation", &request.operation),
        }
    }

    fn validate(&self, handle: &HandleArg, expected_type: &str) -> Result<(), String> {
        if handle.owner != self.owner {
            return Err(protocol_error("cross_owner_handle", "native resource belongs to another worker"));
        }
        if handle.type_name != expected_type {
            return Err(protocol_error("handle_type_mismatch", &handle.type_name));
        }
        match self.handles.get(&handle.id) {
            Some(entry)
                if entry.generation == handle.generation && entry.type_name == expected_type =>
            {
                Ok(())
            }
            _ => Err(protocol_error("stale_handle", "native resource handle is stale")),
        }
    }

    fn live(&self, handle: &HandleArg, expected_type: &str) -> Result<&HandleEntry, String> {
        self.validate(handle, expected_type)?;
        Ok(self.handles.get(&handle.id).expect("validated handle"))
    }

    fn live_mut(
        &mut self,
        handle: &HandleArg,
        expected_type: &str,
    ) -> Result<&mut HandleEntry, String> {
        self.validate(handle, expected_type)?;
        Ok(self.handles.get_mut(&handle.id).expect("validated handle"))
    }
}

fn request_id(line: &str) -> Option<u64> {
    request_id_from_call(line)
}

fn parse_request(line: &str) -> Result<Request, String> {
    let mut fields = line.split_whitespace();
    if fields.next() != Some("call") {
        return Err(protocol_error("invalid_request", "expected call request"));
    }
    let _request_id = fields
        .next()
        .ok_or_else(|| protocol_error("invalid_request", "missing request id"))?
        .parse::<u64>()
        .map_err(|error| protocol_error("invalid_request", &error.to_string()))?;
    let operation = decode_text(fields.next().ok_or_else(|| {
        protocol_error("invalid_request", "missing encoded operation")
    })?)?;
    let args = fields.map(parse_arg).collect::<Result<Vec<_>, _>>()?;
    Ok(Request { operation, args })
}

fn parse_arg(value: &str) -> Result<Arg, String> {
    // The VM uses `ls:` for an untyped empty list. Generated operation patterns
    // resolve it against the declared numeric list argument.
    if value == "ls:" {
        return Ok(Arg::EmptyList);
    }
    if let Some(value) = value.strip_prefix("i:") {
        return value
            .parse::<i64>()
            .map(Arg::Int)
            .map_err(|error| protocol_error("invalid_argument", &error.to_string()));
    }
    if let Some(value) = value.strip_prefix("f:") {
        return value
            .parse::<f64>()
            .map(Arg::Float)
            .map_err(|error| protocol_error("invalid_argument", &error.to_string()));
    }
    if let Some(value) = value.strip_prefix("b:") {
        return value
            .parse::<bool>()
            .map(Arg::Bool)
            .map_err(|error| protocol_error("invalid_argument", &error.to_string()));
    }
    if let Some(value) = value.strip_prefix("li:") {
        if value.is_empty() {
            return Ok(Arg::Ints(Vec::new()));
        }
        return value
            .split(',')
            .map(|value| {
                value.parse::<i64>().map_err(|error| {
                    protocol_error("invalid_argument", &error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Ints);
    }
    if let Some(value) = value.strip_prefix("lf:") {
        if value.is_empty() {
            return Ok(Arg::Floats(Vec::new()));
        }
        return value
            .split(',')
            .map(|value| {
                value.parse::<f64>().map_err(|error| {
                    protocol_error("invalid_argument", &error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Floats);
    }
    if let Some(value) = value.strip_prefix("s:") {
        return decode_text(value).map(Arg::String);
    }
    if let Some(value) = value.strip_prefix("x:") {
        let bytes = STANDARD
            .decode(value)
            .map_err(|error| protocol_error("invalid_base64", &error.to_string()));
        return bytes.and_then(|bytes| {
            if bytes.len() > MAX_ADAPTER_TRANSFER_BYTES {
                Err(protocol_error(
                    "transfer_too_large",
                    "copied byte argument exceeds the native adapter transfer bound",
                ))
            } else {
                Ok(Arg::Bytes(bytes))
            }
        });
    }
    if let Some(value) = value.strip_prefix("a:") {
        return decode_text(value).map(Arg::Atom);
    }
    if let Some(value) = value.strip_prefix("r:") {
        return parse_record(value).map(Arg::Record);
    }
    if let Some(value) = value.strip_prefix("h:") {
        let fields = value.split(':').collect::<Vec<_>>();
        let [owner, id, generation, type_name] = fields.as_slice() else {
            return Err(protocol_error("invalid_argument", "malformed handle"));
        };
        return Ok(Arg::Handle(HandleArg {
            owner: decode_text(owner)?,
            id: id.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
            generation: generation.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
            type_name: decode_text(type_name)?,
        }));
    }
    Err(protocol_error("invalid_argument", "unsupported argument encoding"))
}

fn arg_ints(value: &Arg) -> &[i64] {
    match value {
        Arg::Ints(values) => values,
        Arg::EmptyList => &[],
        _ => unreachable!("generated operation pattern validates integer-list arguments"),
    }
}

fn arg_floats(value: &Arg) -> &[f64] {
    match value {
        Arg::Floats(values) => values,
        Arg::EmptyList => &[],
        _ => unreachable!("generated operation pattern validates float-list arguments"),
    }
}

/// Parses one strict copied-record wire argument.
fn parse_record(value: &str) -> Result<RecordArg, String> {
    let (name, encoded_fields) = value
        .split_once(':')
        .ok_or_else(|| protocol_error("invalid_argument", "malformed copied record"))?;
    let name = decode_text(name)?;
    if name.is_empty() || encoded_fields.is_empty() {
        return Err(protocol_error("invalid_argument", "empty copied record"));
    }
    let mut fields = HashMap::new();
    for encoded_field in encoded_fields.split(',') {
        let parts = encoded_field.splitn(3, ':').collect::<Vec<_>>();
        let [field, kind, value] = parts.as_slice() else {
            return Err(protocol_error("invalid_argument", "malformed copied record field"));
        };
        let field = decode_text(field)?;
        if field.is_empty() || fields.contains_key(&field) {
            return Err(protocol_error("invalid_argument", "empty or duplicate copied record field"));
        }
        let value = match *kind {
            "i" => value
                .parse::<i64>()
                .map(RecordField::Int)
                .map_err(|error| protocol_error("invalid_argument", &error.to_string()))?,
            "f" => value
                .parse::<f64>()
                .map(RecordField::Float)
                .map_err(|error| protocol_error("invalid_argument", &error.to_string()))?,
            "b" => value
                .parse::<bool>()
                .map(RecordField::Bool)
                .map_err(|error| protocol_error("invalid_argument", &error.to_string()))?,
            _ => return Err(protocol_error("invalid_argument", "unsupported copied record field")),
        };
        fields.insert(field, value);
    }
    Ok(RecordArg { name, fields })
}

fn decode_text(value: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|error| protocol_error("invalid_base64", &error.to_string()))?;
    String::from_utf8(bytes).map_err(|error| protocol_error("invalid_utf8", &error.to_string()))
}

fn protocol_error(code: &str, message: &str) -> String {
    format!("err {} {}", STANDARD.encode(code), STANDARD.encode(message))
}
@NULL_FAILURE@
"##;
