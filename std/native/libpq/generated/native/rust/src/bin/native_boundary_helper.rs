#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use terlan_libpq::{CAbiError, Connection, QueryResult};

const MAX_ADAPTER_FRAME_BYTES: usize = 1048576;
const MAX_ADAPTER_TRANSFER_BYTES: usize = 16777216;

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
            Err(error) => (
                protocol_error("native_read_error", &error.to_string()),
                true,
            ),
        };
        if writeln!(stdout, "{payload}").is_err() || stdout.flush().is_err() || terminate {
            break;
        }
    }
}

struct Worker {
    owner: String,
    last_request_id: Option<u64>,
    next_id: u64,
    free_ids: Vec<u64>,
    generations: HashMap<u64, u64>,
    handles: HashMap<u64, HandleEntry>,
}

struct HandleEntry {
    generation: u64,
    value: HandleValue,
}

enum HandleValue {
    Connection(Connection),
    QueryResult(QueryResult),
}

impl HandleValue {
    fn type_name(&self) -> &'static str {
        match self {
            HandleValue::Connection(_) => "terlan_libpq.Driver.Connection",
            HandleValue::QueryResult(_) => "terlan_libpq.Driver.QueryResult",
        }
    }
}

#[derive(Clone)]
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

impl Worker {
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
        let request = match parse_request(line) {
            Ok(request) => request,
            Err(error) => return format!("reply {request_id} 1 {error}"),
        };
        let payload = self.execute(request);
        format!("reply {request_id} 1 {payload}")
    }

    fn execute(&mut self, request: Request) -> String {
        match request.operation.as_str() {
            "postgres.libpq.connection.start" => {
                let [Arg::String(url)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "start expects its declared arguments",
                    );
                };
                let value = match Connection::start(url.as_str()) {
                    Ok(value) => value,
                    Err(error) => return native_error(&error),
                };
                let (id, generation) = match self.store_handle(HandleValue::Connection(value)) {
                    Ok(handle) => handle,
                    Err(error) => return error,
                };
                format!(
                    "ok_handle {} {id} {generation} {}",
                    STANDARD.encode(self.owner.as_bytes()),
                    STANDARD.encode("terlan_libpq.Driver.Connection")
                )
            }
            "postgres.libpq.connection.dispose" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "dispose_connection expects its declared arguments",
                    );
                };
                if let Err(error) = self.validate(connection, "terlan_libpq.Driver.Connection") {
                    return error;
                }
                self.release_handle(connection.id);
                "ok_unit".to_string()
            }
            "postgres.libpq.connection.socket" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "socket expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.socket() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.poll_connect" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "poll_connect expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.poll_connect() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.error_length" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "error_length expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.error_length() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.error_bytes" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "error_bytes expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.error_bytes() {
                    Ok(values) if values.is_empty() => "ok_ints".to_string(),
                    Ok(values) => format!(
                        "ok_ints {}",
                        values
                            .iter()
                            .map(i64::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.clear_parameters" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "clear_parameters expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.clear_parameters() {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.push_null" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "push_null expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.push_null() {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.push_text" => {
                let [Arg::Handle(connection), Arg::String(value)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "push_text expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.push_text(value.as_str()) {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.send_query" => {
                let [Arg::Handle(connection), Arg::String(sql)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "send_query expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.send_query(sql.as_str()) {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.send_batch" => {
                let [Arg::Handle(connection), Arg::String(sql)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "send_batch expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.send_batch(sql.as_str()) {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.consume_input" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "consume_input expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.consume_input() {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.is_busy" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "is_busy expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.is_busy() {
                    Ok(value) => format!("ok_bool {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.connection.next_result" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "next_result expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                let value = match value_connection.next_result() {
                    Ok(value) => value,
                    Err(error) => return native_error(&error),
                };
                let (id, generation) = match self.store_handle(HandleValue::QueryResult(value)) {
                    Ok(handle) => handle,
                    Err(error) => return error,
                };
                format!(
                    "ok_handle {} {id} {generation} {}",
                    STANDARD.encode(self.owner.as_bytes()),
                    STANDARD.encode("terlan_libpq.Driver.QueryResult")
                )
            }
            "postgres.libpq.connection.abort" => {
                let [Arg::Handle(connection)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "abort expects its declared arguments",
                    );
                };
                let value_connection = match self.live_connection_mut(connection) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_connection.abort() {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.dispose" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "dispose_result expects its declared arguments",
                    );
                };
                if let Err(error) = self.validate(result, "terlan_libpq.Driver.QueryResult") {
                    return error;
                }
                self.release_handle(result.id);
                "ok_unit".to_string()
            }
            "postgres.libpq.result.status" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "status expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.status() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.row_count" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "row_count expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.row_count() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.column_count" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "column_count expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.column_count() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.select_column_name" => {
                let [Arg::Handle(result), Arg::Int(column)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "select_column_name expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult_mut(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.select_column_name(*column) {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.column_oid" => {
                let [Arg::Handle(result), Arg::Int(column)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "column_oid expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.column_oid(*column) {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.select_value" => {
                let [Arg::Handle(result), Arg::Int(row), Arg::Int(column)] =
                    request.args.as_slice()
                else {
                    return protocol_error(
                        "invalid_arguments",
                        "select_value expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult_mut(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.select_value(*row, *column) {
                    Ok(()) => "ok_unit".to_string(),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.value_length" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "value_length expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.value_length() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.value_bytes" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "value_bytes expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.value_bytes() {
                    Ok(values) if values.is_empty() => "ok_ints".to_string(),
                    Ok(values) => format!(
                        "ok_ints {}",
                        values
                            .iter()
                            .map(i64::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.value_is_null" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "value_is_null expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.value_is_null() {
                    Ok(value) => format!("ok_bool {value}"),
                    Err(error) => native_error(&error),
                }
            }
            "postgres.libpq.result.affected_rows" => {
                let [Arg::Handle(result)] = request.args.as_slice() else {
                    return protocol_error(
                        "invalid_arguments",
                        "affected_rows expects its declared arguments",
                    );
                };
                let value_result = match self.live_queryresult(result) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                match value_result.affected_rows() {
                    Ok(value) => format!("ok_int {value}"),
                    Err(error) => native_error(&error),
                }
            }

            _ => protocol_error("unknown_operation", &request.operation),
        }
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
            Some(entry) if entry.generation != handle.generation => Err(protocol_error(
                "stale_handle",
                "NativeBoundary handle is stale",
            )),
            Some(entry) if entry.value.type_name() != expected_type => Err(protocol_error(
                "handle_storage_mismatch",
                "NativeBoundary handle resource type does not match",
            )),
            Some(_) => Ok(()),
            _ => Err(protocol_error(
                "stale_handle",
                "NativeBoundary handle is stale",
            )),
        }
    }

    fn live_connection(&self, handle: &HandleArg) -> Result<&Connection, String> {
        self.validate(handle, "terlan_libpq.Driver.Connection")?;
        match &self
            .handles
            .get(&handle.id)
            .expect("validated handle")
            .value
        {
            HandleValue::Connection(value) => Ok(value),
            _ => Err(protocol_error(
                "handle_storage_mismatch",
                "terlan_libpq.Driver.Connection",
            )),
        }
    }

    fn live_connection_mut(&mut self, handle: &HandleArg) -> Result<&mut Connection, String> {
        self.validate(handle, "terlan_libpq.Driver.Connection")?;
        match &mut self
            .handles
            .get_mut(&handle.id)
            .expect("validated handle")
            .value
        {
            HandleValue::Connection(value) => Ok(value),
            _ => Err(protocol_error(
                "handle_storage_mismatch",
                "terlan_libpq.Driver.Connection",
            )),
        }
    }

    fn live_queryresult(&self, handle: &HandleArg) -> Result<&QueryResult, String> {
        self.validate(handle, "terlan_libpq.Driver.QueryResult")?;
        match &self
            .handles
            .get(&handle.id)
            .expect("validated handle")
            .value
        {
            HandleValue::QueryResult(value) => Ok(value),
            _ => Err(protocol_error(
                "handle_storage_mismatch",
                "terlan_libpq.Driver.QueryResult",
            )),
        }
    }

    fn live_queryresult_mut(&mut self, handle: &HandleArg) -> Result<&mut QueryResult, String> {
        self.validate(handle, "terlan_libpq.Driver.QueryResult")?;
        match &mut self
            .handles
            .get_mut(&handle.id)
            .expect("validated handle")
            .value
        {
            HandleValue::QueryResult(value) => Ok(value),
            _ => Err(protocol_error(
                "handle_storage_mismatch",
                "terlan_libpq.Driver.QueryResult",
            )),
        }
    }
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
    let operation = decode_text(
        fields
            .next()
            .ok_or_else(|| protocol_error("invalid_request", "missing encoded operation"))?,
    )?;
    let args = fields.map(parse_arg).collect::<Result<Vec<_>, _>>()?;
    for argument in &args {
        validate_decoded_arg_shape(argument);
    }
    Ok(Request { operation, args })
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
                value
                    .parse::<i64>()
                    .map_err(|error| protocol_error("invalid_argument", &error.to_string()))
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
                value
                    .parse::<f64>()
                    .map_err(|error| protocol_error("invalid_argument", &error.to_string()))
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
                value
                    .parse::<bool>()
                    .map_err(|error| protocol_error("invalid_argument", &error.to_string()))
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
    Err(protocol_error(
        "invalid_argument",
        "unsupported argument encoding",
    ))
}

fn parse_handle_arg(value: &str) -> Result<HandleArg, String> {
    let fields = value.split(':').collect::<Vec<_>>();
    let [owner, id, generation, type_name] = fields.as_slice() else {
        return Err(protocol_error("invalid_argument", "malformed handle"));
    };
    Ok(HandleArg {
        owner: decode_text(owner)?,
        id: id.parse().map_err(|error: std::num::ParseIntError| {
            protocol_error("invalid_argument", &error.to_string())
        })?,
        generation: generation
            .parse()
            .map_err(|error: std::num::ParseIntError| {
                protocol_error("invalid_argument", &error.to_string())
            })?,
        type_name: decode_text(type_name)?,
    })
}

fn decode_text(value: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|error| protocol_error("invalid_base64", &error.to_string()))?;
    String::from_utf8(bytes).map_err(|error| protocol_error("invalid_utf8", &error.to_string()))
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
        Arg::Int(value) => {
            let _ = value;
        }
        Arg::Float(value) => {
            let _ = value;
        }
        Arg::Bool(value) => {
            let _ = value;
        }
        Arg::String(value) => {
            let _ = value;
        }
        Arg::Bytes(value) => {
            let _ = value;
        }
        Arg::Ints(_) => {
            let _ = arg_ints(value);
        }
        Arg::Floats(_) => {
            let _ = arg_floats(value);
        }
        Arg::Bools(_) => {
            let _ = arg_bools(value);
        }
        Arg::Handles(_) => {
            let _ = arg_handles(value);
        }
        Arg::EmptyList => {}
        Arg::Handle(value) => {
            let _ = value;
        }
    }
}

fn native_error(error: &CAbiError) -> String {
    protocol_error(&format!("c_abi_status_{}", error.status), error.operation)
}

fn protocol_error(code: &str, message: &str) -> String {
    format!("err {} {}", STANDARD.encode(code), STANDARD.encode(message))
}
