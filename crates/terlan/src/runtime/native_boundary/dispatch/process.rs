//! Bounded child-process execution for trusted VM tooling capabilities.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;

use super::{DispatchError, NativeBoundaryValue};

mod framed;
use framed::{FrameSignal, FramedProcessRequest};

const MAX_TIMEOUT_MS: u64 = 3_600_000;
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_STDIN_BYTES: usize = 1024 * 1024;
const MAX_BATCH_COMMANDS: usize = 4096;
const MAX_BATCH_CONCURRENCY: usize = 64;
const MAX_FRAMED_EXCHANGES: usize = 4096;
const MAX_FRAMED_RESPONSES: usize = 4096;
const MAX_FRAME_HEADER_BYTES: usize = 64 * 1024;

/// Returns the authoritative typed process resource ceilings.
pub(super) fn process_limits() -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "Limits".to_string(),
        fields: vec![
            (
                "maximum_timeout_ms".to_string(),
                NativeBoundaryValue::Int(MAX_TIMEOUT_MS as i64),
            ),
            (
                "maximum_output_bytes".to_string(),
                NativeBoundaryValue::Int(MAX_OUTPUT_BYTES as i64),
            ),
            (
                "maximum_stdin_bytes".to_string(),
                NativeBoundaryValue::Int(MAX_STDIN_BYTES as i64),
            ),
            (
                "maximum_batch_commands".to_string(),
                NativeBoundaryValue::Int(MAX_BATCH_COMMANDS as i64),
            ),
            (
                "maximum_batch_concurrency".to_string(),
                NativeBoundaryValue::Int(MAX_BATCH_CONCURRENCY as i64),
            ),
            (
                "maximum_framed_exchanges".to_string(),
                NativeBoundaryValue::Int(MAX_FRAMED_EXCHANGES as i64),
            ),
            (
                "maximum_framed_responses".to_string(),
                NativeBoundaryValue::Int(MAX_FRAMED_RESPONSES as i64),
            ),
        ],
    }
}

pub(super) fn run_process(
    args: &[NativeBoundaryValue],
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryValue, DispatchError> {
    let request = match parse_request(args) {
        Ok(request) => request,
        Err((message, program)) => return Ok(process_error("invalid_request", message, program)),
    };
    execute(request, cancellation)
}

/// Executes an ordered command batch with bounded worker concurrency.
///
/// Every command retains its own typed completion or request error. Only an
/// invalid batch shape/bound or an internal dispatch failure rejects the outer
/// result. Worker completion order never changes the returned list order.
pub(super) fn run_process_many(
    args: &[NativeBoundaryValue],
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryValue, DispatchError> {
    let Some(NativeBoundaryValue::Record { fields, .. }) = args.first() else {
        return Ok(process_error(
            "invalid_request",
            "process batch request must be BatchRequest",
            "",
        ));
    };
    let commands = match field(fields, "commands") {
        Ok(NativeBoundaryValue::List(commands)) => commands,
        _ => {
            return Ok(process_error(
                "invalid_request",
                "process batch commands must be List[Command]",
                "",
            ))
        }
    };
    if commands.len() > MAX_BATCH_COMMANDS {
        return Ok(process_error(
            "invalid_request",
            format!("process batch exceeds {MAX_BATCH_COMMANDS} commands"),
            "",
        ));
    }
    let Ok(NativeBoundaryValue::Int(max_concurrency)) = field(fields, "max_concurrency") else {
        return Ok(process_error(
            "invalid_request",
            "process batch max_concurrency must be Int",
            "",
        ));
    };
    let Some(max_concurrency) = usize::try_from(*max_concurrency)
        .ok()
        .filter(|value| (1..=MAX_BATCH_CONCURRENCY).contains(value))
    else {
        return Ok(process_error(
            "invalid_request",
            format!("process batch max_concurrency must be between 1 and {MAX_BATCH_CONCURRENCY}"),
            "",
        ));
    };

    enum BatchItem {
        Request(ProcessRequest),
        Ready(NativeBoundaryValue),
    }

    let mut pending = VecDeque::with_capacity(commands.len());
    for (index, command) in commands.iter().enumerate() {
        let item = match parse_request(std::slice::from_ref(command)) {
            Ok(request) => BatchItem::Request(request),
            Err((message, program)) => {
                BatchItem::Ready(process_error("invalid_request", message, program))
            }
        };
        pending.push_back((index, item));
    }
    if pending.is_empty() {
        return Ok(process_batch_output(Vec::new()));
    }

    let result_count = pending.len();
    let pending = Mutex::new(pending);
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..max_concurrency.min(result_count) {
            let sender = sender.clone();
            let pending = &pending;
            scope.spawn(move || loop {
                let next = pending.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((index, item)) = next else {
                    return;
                };
                let result = match item {
                    BatchItem::Request(request) => execute(request, cancellation),
                    BatchItem::Ready(value) => Ok(value),
                };
                if sender.send((index, result)).is_err() {
                    return;
                }
            });
        }
    });
    drop(sender);

    let mut ordered: Vec<Option<Result<NativeBoundaryValue, DispatchError>>> =
        (0..result_count).map(|_| None).collect();
    for (index, result) in receiver {
        ordered[index] = Some(result);
    }
    let mut values = Vec::with_capacity(result_count);
    for result in ordered {
        let result = result.ok_or_else(|| {
            DispatchError::new(
                "process.batch",
                "process batch worker ended without returning a result",
                0,
            )
        })?;
        values.push(result?);
    }
    Ok(process_batch_output(values))
}

pub(super) use framed::run_process_length_framed;

struct ProcessRequest {
    program: String,
    arguments: Vec<String>,
    working_directory: Option<String>,
    environment: Vec<(String, String)>,
    removed_environment: Vec<String>,
    stdin: Vec<u8>,
    timeout: Duration,
    output_limit: usize,
}

enum ProcessCompletion {
    Status(std::process::ExitStatus),
    Failure(&'static str, String),
}

#[derive(Clone, Debug)]
struct FramedReadError(String);

impl std::fmt::Display for FramedReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FramedReadError {}

impl From<String> for FramedReadError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for FramedReadError {
    fn from(message: &str) -> Self {
        Self(message.to_string())
    }
}

fn parse_request(args: &[NativeBoundaryValue]) -> Result<ProcessRequest, (String, String)> {
    let Some(NativeBoundaryValue::Record { fields, .. }) = args.first() else {
        return Err(("process request must be Command".to_string(), String::new()));
    };
    let program = text_field(fields, "program")?.to_string();
    if program.is_empty() {
        return Err(("program must not be empty".to_string(), program));
    }
    let arguments = text_list_field(fields, "arguments", &program)?;
    let working_directory = optional_text_field(fields, "working_directory", &program)?;
    let environment = environment_field(fields, &program)?;
    let removed_environment = text_list_field(fields, "removed_environment", &program)?;
    let stdin = text_field(fields, "stdin")?.as_bytes().to_vec();
    if stdin.len() > MAX_STDIN_BYTES {
        return Err((format!("stdin exceeds {MAX_STDIN_BYTES} bytes"), program));
    }
    let timeout_ms = positive_usize_field(fields, "timeout_ms", MAX_TIMEOUT_MS as usize)?;
    let output_limit = positive_usize_field(fields, "output_limit_bytes", MAX_OUTPUT_BYTES)?;
    Ok(ProcessRequest {
        program,
        arguments,
        working_directory,
        environment,
        removed_environment,
        stdin,
        timeout: Duration::from_millis(timeout_ms as u64),
        output_limit,
    })
}

fn execute(
    request: ProcessRequest,
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryValue, DispatchError> {
    let mut command = Command::new(&request.program);
    command
        .args(&request.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = &request.working_directory {
        command.current_dir(directory);
    }
    for key in &request.removed_environment {
        command.env_remove(key);
    }
    for (key, value) in &request.environment {
        command.env(key, value);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(process_error(
                "spawn_failed",
                format!("cannot spawn child: {error}"),
                request.program,
            ))
        }
    };

    let input = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stdout pipe is unavailable", 0))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stderr pipe is unavailable", 0))?;
    let input_bytes = request.stdin;
    let input_thread = std::thread::spawn(move || {
        if let Some(mut input) = input {
            let _ = input.write_all(&input_bytes);
        }
    });
    let total = Arc::new(AtomicUsize::new(0));
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout_thread = read_bounded(stdout, request.output_limit, &total, &overflow);
    let stderr_thread = read_bounded(stderr, request.output_limit, &total, &overflow);
    let started = Instant::now();
    let completion = loop {
        if cancellation.is_some_and(NativeBoundaryCancellationToken::is_cancelled) {
            break ProcessCompletion::Failure(
                "cancelled",
                "child process was cancelled".to_string(),
            );
        }
        if overflow.load(Ordering::Acquire) {
            break ProcessCompletion::Failure(
                "output_limit_exceeded",
                "child output exceeded its byte limit".to_string(),
            );
        }
        if started.elapsed() >= request.timeout {
            break ProcessCompletion::Failure(
                "timed_out",
                "child process exceeded its wall-clock timeout".to_string(),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => break ProcessCompletion::Status(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                join_input(input_thread);
                let _ = join_output(stdout_thread);
                let _ = join_output(stderr_thread);
                return Ok(process_error(
                    "spawn_failed",
                    format!("cannot wait for child: {error}"),
                    request.program,
                ));
            }
        }
    };

    let status = match completion {
        ProcessCompletion::Failure(code, message) => {
            let _ = child.kill();
            let _ = child.wait();
            join_input(input_thread);
            let _ = join_output(stdout_thread);
            let _ = join_output(stderr_thread);
            return Ok(process_error(code, message, request.program));
        }
        ProcessCompletion::Status(status) => status,
    };
    join_input(input_thread);
    let stdout = join_output(stdout_thread)?;
    let stderr = join_output(stderr_thread)?;
    Ok(process_output(
        status.code().map(i64::from).unwrap_or(-1),
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

fn execute_length_framed(
    request: FramedProcessRequest,
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryValue, DispatchError> {
    let mut command = configured_command(&request.command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(process_error(
                "spawn_failed",
                format!("cannot spawn child: {error}"),
                request.command.program,
            ))
        }
    };

    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stdin pipe is unavailable", 0))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stdout pipe is unavailable", 0))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stderr pipe is unavailable", 0))?;
    let total = Arc::new(AtomicUsize::new(0));
    let overflow = Arc::new(AtomicBool::new(false));
    let (frame_sender, frame_receiver) = mpsc::channel();
    let stdout_thread = read_length_framed(
        stdout,
        request.length_header,
        request.command.output_limit,
        &total,
        &overflow,
        frame_sender,
    );
    let stderr_thread = read_bounded(stderr, request.command.output_limit, &total, &overflow);
    let started = Instant::now();

    let mut failure = write_framed_input(&mut input, &request.command.stdin).err();
    for exchange in &request.exchanges {
        if failure.is_some() {
            break;
        }
        if let Err(error) = write_framed_input(&mut input, &exchange.input) {
            failure = Some(error);
            break;
        }
        for _ in 0..exchange.expected_frames {
            if let Err(error) = await_frame(
                &frame_receiver,
                started,
                request.command.timeout,
                &overflow,
                cancellation,
            ) {
                failure = Some(error);
                break;
            }
        }
    }
    drop(input);

    let completion = if let Some((code, message)) = failure {
        ProcessCompletion::Failure(code, message)
    } else {
        wait_for_child(
            &mut child,
            started,
            request.command.timeout,
            &overflow,
            cancellation,
        )
    };

    let status = match completion {
        ProcessCompletion::Failure(code, message) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_framed_output(stdout_thread);
            let _ = join_output(stderr_thread);
            return Ok(process_error(code, message, request.command.program));
        }
        ProcessCompletion::Status(status) => status,
    };
    let frames = match join_framed_output(stdout_thread) {
        Ok(frames) => frames,
        Err(message) => {
            let _ = join_output(stderr_thread);
            return Ok(process_error(
                "invalid_frame",
                message.to_string(),
                request.command.program,
            ));
        }
    };
    let stderr = join_output(stderr_thread)?;
    if overflow.load(Ordering::Acquire) {
        return Ok(process_error(
            "output_limit_exceeded",
            "child output exceeded its byte limit",
            request.command.program,
        ));
    }
    Ok(framed_process_output(
        status.code().map(i64::from).unwrap_or(-1),
        frames,
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

fn configured_command(request: &ProcessRequest) -> Command {
    let mut command = Command::new(&request.program);
    command
        .args(&request.arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = &request.working_directory {
        command.current_dir(directory);
    }
    for key in &request.removed_environment {
        command.env_remove(key);
    }
    for (key, value) in &request.environment {
        command.env(key, value);
    }
    command
}

fn write_framed_input(input: &mut impl Write, bytes: &[u8]) -> Result<(), (&'static str, String)> {
    input
        .write_all(bytes)
        .and_then(|()| input.flush())
        .map_err(|error| {
            (
                "invalid_frame",
                format!("cannot write framed child input: {error}"),
            )
        })
}

fn await_frame(
    receiver: &mpsc::Receiver<FrameSignal>,
    started: Instant,
    timeout: Duration,
    overflow: &AtomicBool,
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<(), (&'static str, String)> {
    loop {
        if cancellation.is_some_and(NativeBoundaryCancellationToken::is_cancelled) {
            return Err(("cancelled", "child process was cancelled".to_string()));
        }
        if overflow.load(Ordering::Acquire) {
            return Err((
                "output_limit_exceeded",
                "child output exceeded its byte limit".to_string(),
            ));
        }
        let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
            return Err((
                "timed_out",
                "child process exceeded its wall-clock timeout".to_string(),
            ));
        };
        match receiver.recv_timeout(remaining.min(Duration::from_millis(5))) {
            Ok(FrameSignal::Frame) => return Ok(()),
            Ok(FrameSignal::End) => {
                return Err((
                    "invalid_frame",
                    "child stdout ended before the expected response frame".to_string(),
                ))
            }
            Ok(FrameSignal::Invalid(message)) => return Err(("invalid_frame", message)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err((
                    "invalid_frame",
                    "framed child stdout reader ended unexpectedly".to_string(),
                ))
            }
        }
    }
}

fn wait_for_child(
    child: &mut std::process::Child,
    started: Instant,
    timeout: Duration,
    overflow: &AtomicBool,
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> ProcessCompletion {
    loop {
        if cancellation.is_some_and(NativeBoundaryCancellationToken::is_cancelled) {
            return ProcessCompletion::Failure(
                "cancelled",
                "child process was cancelled".to_string(),
            );
        }
        if overflow.load(Ordering::Acquire) {
            return ProcessCompletion::Failure(
                "output_limit_exceeded",
                "child output exceeded its byte limit".to_string(),
            );
        }
        if started.elapsed() >= timeout {
            return ProcessCompletion::Failure(
                "timed_out",
                "child process exceeded its wall-clock timeout".to_string(),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => return ProcessCompletion::Status(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(error) => {
                return ProcessCompletion::Failure(
                    "spawn_failed",
                    format!("cannot wait for child: {error}"),
                )
            }
        }
    }
}

fn read_length_framed(
    reader: impl Read + Send + 'static,
    length_header: String,
    limit: usize,
    total: &Arc<AtomicUsize>,
    overflow: &Arc<AtomicBool>,
    sender: mpsc::Sender<FrameSignal>,
) -> std::thread::JoinHandle<Result<Vec<String>, FramedReadError>> {
    let total = Arc::clone(total);
    let overflow = Arc::clone(overflow);
    std::thread::spawn(move || {
        let result =
            collect_length_framed(reader, &length_header, limit, &total, &overflow, &sender);
        match &result {
            Ok(_) => {
                let _ = sender.send(FrameSignal::End);
            }
            Err(message) => {
                let _ = sender.send(FrameSignal::Invalid(message.to_string()));
            }
        }
        result
    })
}

fn collect_length_framed(
    reader: impl Read,
    length_header: &str,
    limit: usize,
    total: &AtomicUsize,
    overflow: &AtomicBool,
    sender: &mpsc::Sender<FrameSignal>,
) -> Result<Vec<String>, FramedReadError> {
    let mut reader = BufReader::new(reader);
    let mut frames = Vec::new();
    loop {
        let mut header_bytes = 0usize;
        let mut body_length = None;
        loop {
            let mut line = Vec::new();
            let count = reader
                .read_until(b'\n', &mut line)
                .map_err(|error| format!("cannot read framed child header: {error}"))?;
            if count == 0 {
                if header_bytes == 0 {
                    return Ok(frames);
                }
                return Err("child stdout ended inside a frame header".into());
            }
            account_output(count, limit, total, overflow);
            header_bytes = header_bytes.saturating_add(count);
            if header_bytes > MAX_FRAME_HEADER_BYTES {
                return Err(
                    format!("framed child header exceeds {MAX_FRAME_HEADER_BYTES} bytes").into(),
                );
            }
            if line == b"\r\n" || line == b"\n" {
                break;
            }
            let line = std::str::from_utf8(&line)
                .map_err(|_| "framed child header is not UTF-8".to_string())?
                .trim_end_matches(['\r', '\n']);
            let Some((name, value)) = line.split_once(':') else {
                return Err("framed child header line is missing `:`".into());
            };
            if name.trim().eq_ignore_ascii_case(length_header) {
                if body_length.is_some() {
                    return Err(format!("framed child repeated `{length_header}` header").into());
                }
                body_length = Some(value.trim().parse::<usize>().map_err(|_| {
                    format!("framed child `{length_header}` header is not a byte count")
                })?);
            }
        }
        let body_length = body_length
            .ok_or_else(|| format!("framed child response is missing `{length_header}` header"))?;
        if body_length > limit {
            overflow.store(true, Ordering::Release);
            return Err("child output exceeded its byte limit".into());
        }
        let mut body = vec![0_u8; body_length];
        reader
            .read_exact(&mut body)
            .map_err(|error| format!("cannot read framed child body: {error}"))?;
        account_output(body_length, limit, total, overflow);
        let body =
            String::from_utf8(body).map_err(|_| "framed child body is not UTF-8".to_string())?;
        frames.push(body);
        if sender.send(FrameSignal::Frame).is_err() {
            return Ok(frames);
        }
    }
}

fn account_output(count: usize, limit: usize, total: &AtomicUsize, overflow: &AtomicBool) {
    let previous = total.fetch_add(count, Ordering::AcqRel);
    if previous.saturating_add(count) > limit {
        overflow.store(true, Ordering::Release);
    }
}

fn join_framed_output(
    thread: std::thread::JoinHandle<Result<Vec<String>, FramedReadError>>,
) -> Result<Vec<String>, FramedReadError> {
    thread
        .join()
        .map_err(|_| "framed child stdout reader panicked".to_string())?
}

fn read_bounded(
    mut reader: impl Read + Send + 'static,
    limit: usize,
    total: &Arc<AtomicUsize>,
    overflow: &Arc<AtomicBool>,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    let total = Arc::clone(total);
    let overflow = Arc::clone(overflow);
    std::thread::spawn(move || {
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                return Ok(captured);
            }
            let previous = total.fetch_add(count, Ordering::AcqRel);
            if previous.saturating_add(count) > limit {
                overflow.store(true, Ordering::Release);
            }
            let remaining = limit.saturating_sub(previous);
            captured.extend_from_slice(&buffer[..count.min(remaining)]);
        }
    })
}

fn join_input(thread: std::thread::JoinHandle<()>) {
    let _ = thread.join();
}

fn join_output(
    thread: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, DispatchError> {
    thread
        .join()
        .map_err(|_| DispatchError::new("process.pipe", "child pipe reader panicked", 0))?
        .map_err(|error| {
            DispatchError::new(
                "process.pipe",
                format!("cannot read child output: {error}"),
                0,
            )
        })
}

fn field<'a>(
    fields: &'a [(String, NativeBoundaryValue)],
    name: &str,
) -> Result<&'a NativeBoundaryValue, (String, String)> {
    fields
        .iter()
        .find(|(field, _)| field == name)
        .map(|(_, value)| value)
        .ok_or_else(|| (format!("Command is missing `{name}`"), String::new()))
}

fn text_field<'a>(
    fields: &'a [(String, NativeBoundaryValue)],
    name: &str,
) -> Result<&'a str, (String, String)> {
    match field(fields, name)? {
        NativeBoundaryValue::Text(value) => Ok(value),
        _ => Err((format!("Command `{name}` must be String"), String::new())),
    }
}

fn text_list_field(
    fields: &[(String, NativeBoundaryValue)],
    name: &str,
    program: &str,
) -> Result<Vec<String>, (String, String)> {
    let NativeBoundaryValue::List(values) = field(fields, name)? else {
        return Err((
            format!("Command `{name}` must be List[String]"),
            program.into(),
        ));
    };
    values
        .iter()
        .map(|value| match value {
            NativeBoundaryValue::Text(value) => Ok(value.clone()),
            _ => Err((
                format!("Command `{name}` must contain only String values"),
                program.into(),
            )),
        })
        .collect()
}

fn optional_text_field(
    fields: &[(String, NativeBoundaryValue)],
    name: &str,
    program: &str,
) -> Result<Option<String>, (String, String)> {
    match field(fields, name)? {
        NativeBoundaryValue::OptionalText(value) => Ok(value.clone()),
        NativeBoundaryValue::Record { name, fields } if name == "None" && fields.is_empty() => {
            Ok(None)
        }
        NativeBoundaryValue::Record { name, fields } if name == "Some" => {
            let value = text_field(fields, "value")?;
            Ok(Some(value.to_string()))
        }
        _ => Err((
            format!("Command `{name}` must be Option[String]"),
            program.into(),
        )),
    }
}

fn environment_field(
    fields: &[(String, NativeBoundaryValue)],
    program: &str,
) -> Result<Vec<(String, String)>, (String, String)> {
    let NativeBoundaryValue::List(entries) = field(fields, "environment")? else {
        return Err((
            "Command `environment` must be List[EnvironmentEntry]".to_string(),
            program.into(),
        ));
    };
    entries
        .iter()
        .map(|entry| {
            let NativeBoundaryValue::Record { fields, .. } = entry else {
                return Err((
                    "Command environment must contain EnvironmentEntry values".to_string(),
                    program.into(),
                ));
            };
            Ok((
                text_field(fields, "key")?.to_string(),
                text_field(fields, "value")?.to_string(),
            ))
        })
        .collect()
}

fn positive_usize_field(
    fields: &[(String, NativeBoundaryValue)],
    name: &str,
    maximum: usize,
) -> Result<usize, (String, String)> {
    let NativeBoundaryValue::Int(value) = field(fields, name)? else {
        return Err((format!("Command `{name}` must be Int"), String::new()));
    };
    let value = usize::try_from(*value)
        .ok()
        .filter(|value| *value > 0 && *value <= maximum)
        .ok_or_else(|| {
            (
                format!("Command `{name}` must be between 1 and {maximum}"),
                String::new(),
            )
        })?;
    Ok(value)
}

fn process_output(status: i64, stdout: String, stderr: String) -> NativeBoundaryValue {
    result_record(
        "Ok",
        "value",
        NativeBoundaryValue::Record {
            name: "Output".to_string(),
            fields: vec![
                ("status".to_string(), NativeBoundaryValue::Int(status)),
                ("stdout".to_string(), NativeBoundaryValue::Text(stdout)),
                ("stderr".to_string(), NativeBoundaryValue::Text(stderr)),
            ],
        },
    )
}

fn framed_process_output(status: i64, frames: Vec<String>, stderr: String) -> NativeBoundaryValue {
    result_record(
        "Ok",
        "value",
        NativeBoundaryValue::Record {
            name: "FramedOutput".to_string(),
            fields: vec![
                ("status".to_string(), NativeBoundaryValue::Int(status)),
                (
                    "frames".to_string(),
                    NativeBoundaryValue::List(
                        frames.into_iter().map(NativeBoundaryValue::Text).collect(),
                    ),
                ),
                ("stderr".to_string(), NativeBoundaryValue::Text(stderr)),
            ],
        },
    )
}

fn process_error(
    code: &str,
    message: impl Into<String>,
    program: impl Into<String>,
) -> NativeBoundaryValue {
    result_record(
        "Err",
        "reason",
        NativeBoundaryValue::Record {
            name: "ProcessError".to_string(),
            fields: vec![
                (
                    "code".to_string(),
                    NativeBoundaryValue::Atom(code.to_string()),
                ),
                (
                    "message".to_string(),
                    NativeBoundaryValue::Text(message.into()),
                ),
                (
                    "program".to_string(),
                    NativeBoundaryValue::Text(program.into()),
                ),
            ],
        },
    )
}

fn process_batch_output(values: Vec<NativeBoundaryValue>) -> NativeBoundaryValue {
    let completions = values.into_iter().map(batch_completion).collect::<Vec<_>>();
    result_record(
        "Ok",
        "value",
        NativeBoundaryValue::Record {
            name: "BatchOutput".to_string(),
            fields: vec![(
                "completions".to_string(),
                NativeBoundaryValue::List(completions),
            )],
        },
    )
}

fn batch_completion(value: NativeBoundaryValue) -> NativeBoundaryValue {
    let (output, error) = match value {
        NativeBoundaryValue::Record { name, mut fields } if name == "Ok" => {
            let value = fields
                .pop()
                .map(|(_, value)| value)
                .unwrap_or(NativeBoundaryValue::Unit);
            (some_record(value), none_record())
        }
        NativeBoundaryValue::Record { name, mut fields } if name == "Err" => {
            let value = fields
                .pop()
                .map(|(_, value)| value)
                .unwrap_or(NativeBoundaryValue::Unit);
            (none_record(), some_record(value))
        }
        _ => (none_record(), none_record()),
    };
    NativeBoundaryValue::Record {
        name: "BatchCompletion".to_string(),
        fields: vec![("output".to_string(), output), ("error".to_string(), error)],
    }
}

fn some_record(value: NativeBoundaryValue) -> NativeBoundaryValue {
    result_record("Some", "value", value)
}

fn none_record() -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "None".to_string(),
        fields: Vec::new(),
    }
}

fn result_record(name: &str, field: &str, value: NativeBoundaryValue) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: name.to_string(),
        fields: vec![(field.to_string(), value)],
    }
}
