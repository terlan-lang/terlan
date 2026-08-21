pub(super) const NATIVE_HELPER_TEMPLATE: &str = r##"#![forbid(unsafe_code)]

@DISPATCH_MODULES@use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use @CRATE@::{@IMPORTS@};

const MAX_ADAPTER_FRAME_BYTES: usize = @MAX_FRAME_BYTES@;
const MAX_ADAPTER_TRANSFER_BYTES: usize = @MAX_TRANSFER_BYTES@;

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
                Ok(line) => (
                    worker.execute_line(line.trim_end_matches(['\r', '\n'])),
                    false,
                ),
                Err(error) => (protocol_error("invalid_utf8", &error.to_string()), false),
            },
            Err(error) => (protocol_error("native_read_error", &error.to_string()), true),
        };
        if writeln!(stdout, "{payload}").is_err() || stdout.flush().is_err() || terminate {
            break;
        }
    }
}

@RESOURCE_DEAD_CODE@struct Worker {
    owner: String,
    last_request_id: Option<u64>,
    next_id: u64,
    free_ids: Vec<u64>,
    generations: HashMap<u64, u64>,
    handles: HashMap<u64, HandleEntry>,
    request_handles: HashMap<u64, HandleArg>,
}

@RESOURCE_DEAD_CODE@struct HandleEntry {
    generation: u64,
    value: HandleValue,
}

@RESOURCE_DEAD_CODE@enum HandleValue {
@HANDLE_VARIANTS@
}

@RESOURCE_DEAD_CODE@impl HandleValue {
    fn type_name(&self) -> &'static str {
        match self {
@HANDLE_TYPE_ARMS@
        }
    }
}

@RESOURCE_DEAD_CODE@#[derive(Clone)]
struct HandleArg {
    owner: String,
    id: u64,
    generation: u64,
    type_name: String,
}

enum Arg {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Bools(Vec<bool>),
    Handles(Vec<HandleArg>),
    EmptyList,
    Handle(HandleArg),
}

struct Request {
    operation: String,
    args: Vec<Arg>,
}

@RESOURCE_DEAD_CODE@impl Worker {
    fn new() -> Result<Self, getrandom::Error> {
        let mut owner = [0_u8; 32];
        getrandom::fill(&mut owner)?;
        Ok(Self {
            owner: STANDARD.encode(owner),
            last_request_id: None,
            next_id: 0,
            free_ids: Vec::new(),
            generations: HashMap::new(),
            handles: HashMap::new(),
            request_handles: HashMap::new(),
        })
    }

    fn store_handle(&mut self, value: HandleValue) -> Result<(u64, u64), String> {
        while let Some(id) = self.free_ids.pop() {
            let previous = self.generations.get(&id).copied().unwrap_or_default();
            if let Some(generation) = previous.checked_add(1) {
                self.generations.insert(id, generation);
                self.handles.insert(id, HandleEntry { generation, value });
                return Ok((id, generation));
            }
        }
        let Some(id) = self.next_id.checked_add(1) else {
            return Err(protocol_error(
                "resource_table_exhausted",
                "NativeBoundary handle table exhausted",
            ));
        };
        self.next_id = id;
        let generation = 1;
        self.generations.insert(id, generation);
        self.handles.insert(id, HandleEntry { generation, value });
        Ok((id, generation))
    }

    fn release_handle(&mut self, id: u64) {
        if self.handles.remove(&id).is_some() {
            self.free_ids.push(id);
        }
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
        let resolved = match resolve_request_references(line, &self.request_handles) {
            Ok(resolved) => resolved,
            Err(error) => return format!("reply {request_id} 1 {error}"),
        };
        let request = match parse_request(&resolved) {
            Ok(request) => request,
            Err(error) => return format!("reply {request_id} 1 {error}"),
        };
        let payload = self.execute(request);
        if let Some(handle) = response_handle(&payload) {
            self.request_handles.insert(request_id, handle);
        }
        format!("reply {request_id} 1 {payload}")
    }

    fn execute(&mut self, request: Request) -> String {
@DISPATCH_CALLS@
    }

    fn validate(&self, handle: &HandleArg, expected_type: &str) -> Result<(), String> {
        if handle.owner != self.owner {
            return Err(protocol_error(
                "cross_owner_handle",
                "native resource belongs to another worker",
            ));
        }
        if handle.type_name != expected_type {
            return Err(protocol_error("handle_type_mismatch", &handle.type_name));
        }
        match self.handles.get(&handle.id) {
            Some(entry) if entry.generation != handle.generation => {
                Err(protocol_error("stale_handle", "NativeBoundary handle is stale"))
            }
            Some(entry) if entry.value.type_name() != expected_type => Err(protocol_error(
                "handle_storage_mismatch",
                "NativeBoundary handle resource type does not match",
            )),
            Some(_) => Ok(()),
            _ => Err(protocol_error("stale_handle", "NativeBoundary handle is stale")),
        }
    }

@HANDLE_ACCESSORS@
}

fn request_id(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace();
    (fields.next() == Some("call"))
        .then(|| fields.next()?.parse::<u64>().ok())
        .flatten()
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
    for argument in &args {
        validate_decoded_arg_shape(argument);
    }
    Ok(Request { operation, args })
}

fn resolve_request_references(
    line: &str,
    request_handles: &HashMap<u64, HandleArg>,
) -> Result<String, String> {
    line.split_whitespace()
        .map(|field| {
            let Some(reference) = field.strip_prefix("r:") else {
                return Ok(field.to_string());
            };
            let parts = reference.split(':').collect::<Vec<_>>();
            let (request_id, overridden_type) = match parts.as_slice() {
                [request_id] => (*request_id, None),
                [request_id, type_name] => (*request_id, Some(*type_name)),
                _ => {
                    return Err(protocol_error(
                        "invalid_handle_reference",
                        "malformed prior-request handle reference",
                    ));
                }
            };
            let request_id = request_id.parse::<u64>().map_err(|error| {
                protocol_error("invalid_handle_reference", &error.to_string())
            })?;
            let handle = request_handles.get(&request_id).ok_or_else(|| {
                protocol_error(
                    "unknown_handle_reference",
                    "prior request did not return a handle",
                )
            })?;
            let type_name = overridden_type
                .map(str::to_string)
                .unwrap_or_else(|| STANDARD.encode(handle.type_name.as_bytes()));
            Ok(format!(
                "h:{}:{}:{}:{type_name}",
                STANDARD.encode(handle.owner.as_bytes()),
                handle.id,
                handle.generation,
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|fields| fields.join(" "))
}

fn response_handle(payload: &str) -> Option<HandleArg> {
    let fields = payload.split_whitespace().collect::<Vec<_>>();
    let ["ok_handle", owner, id, generation, type_name] = fields.as_slice() else {
        return None;
    };
    parse_handle_arg(&format!("{owner}:{id}:{generation}:{type_name}")).ok()
}

fn parse_arg(value: &str) -> Result<Arg, String> {
    // The VM uses `ls:` for an empty list. Preserve the empty value here and
    // resolve it against each generated operation's declared list type.
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
    if let Some(value) = value.strip_prefix("s:") {
        return decode_text(value).map(Arg::String);
    }
    if let Some(value) = value.strip_prefix("x:") {
        let bytes = STANDARD
            .decode(value)
            .map_err(|error| protocol_error("invalid_base64", &error.to_string()))?;
        if bytes.len() > MAX_ADAPTER_TRANSFER_BYTES {
            return Err(protocol_error(
                "transfer_too_large",
                "copied byte argument exceeds the native adapter transfer bound",
            ));
        }
        return Ok(Arg::Bytes(bytes));
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
    if let Some(value) = value.strip_prefix("lb:") {
        if value.is_empty() {
            return Ok(Arg::Bools(Vec::new()));
        }
        return value
            .split(',')
            .map(|value| {
                value.parse::<bool>().map_err(|error| {
                    protocol_error("invalid_argument", &error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Bools);
    }
    if let Some(value) = value.strip_prefix("lh:") {
        if value.is_empty() {
            return Ok(Arg::Handles(Vec::new()));
        }
        return value
            .split(',')
            .map(parse_handle_arg)
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Handles);
    }
    if let Some(value) = value.strip_prefix("h:") {
        return parse_handle_arg(value).map(Arg::Handle);
    }
    Err(protocol_error("invalid_argument", "unsupported argument encoding"))
}

fn parse_handle_arg(value: &str) -> Result<HandleArg, String> {
    let fields = value.split(':').collect::<Vec<_>>();
    let [owner, id, generation, type_name] = fields.as_slice() else {
        return Err(protocol_error("invalid_argument", "malformed handle"));
    };
    Ok(HandleArg {
        owner: decode_text(owner)?,
        id: id.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
        generation: generation.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
        type_name: decode_text(type_name)?,
    })
}

fn decode_text(value: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|error| protocol_error("invalid_base64", &error.to_string()))?;
    String::from_utf8(bytes)
        .map_err(|error| protocol_error("invalid_utf8", &error.to_string()))
}

fn arg_ints(value: &Arg) -> &[i64] {
    match value {
        Arg::Ints(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only integer lists"),
    }
}

fn arg_floats(value: &Arg) -> &[f64] {
    match value {
        Arg::Floats(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only float lists"),
    }
}

fn arg_bools(value: &Arg) -> &[bool] {
    match value {
        Arg::Bools(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only boolean lists"),
    }
}

fn arg_handles(value: &Arg) -> &[HandleArg] {
    match value {
        Arg::Handles(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only resource lists"),
    }
}

fn validate_decoded_arg_shape(value: &Arg) {
    match value {
        Arg::Int(value) => { let _ = value; }
        Arg::Float(value) => { let _ = value; }
        Arg::Bool(value) => { let _ = value; }
        Arg::String(value) => { let _ = value; }
        Arg::Bytes(value) => { let _ = value; }
        Arg::Ints(_) => { let _ = arg_ints(value); }
        Arg::Floats(_) => { let _ = arg_floats(value); }
        Arg::Bools(_) => { let _ = arg_bools(value); }
        Arg::Handles(_) => { let _ = arg_handles(value); }
        Arg::EmptyList => {}
        Arg::Handle(value) => { let _ = value; }
    }
}

@FALLIBLE_DEAD_CODE@fn native_error(error: &CAbiError) -> String {
    protocol_error(
        &format!("c_abi_status_{}", error.status),
        error.operation,
    )
}

fn protocol_error(code: &str, message: &str) -> String {
    format!("err {} {}", STANDARD.encode(code), STANDARD.encode(message))
}
"##;
