//! Bounded authenticated local requests to a live in-memory coverage owner.

use crate::coverage_requests::failure;
use crate::PhaseFailure;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_MESSAGE: u64 = 128 * 1024;
const MAX_REQUESTS: usize = 4096;

/// Owns the listener and joins its bounded request thread on every exit path.
pub(super) struct Bridge {
    endpoint: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Value, PhaseFailure>>>,
}

impl Bridge {
    /// Keeps the admitted test inventory in one owner instead of reparsing it per gate.
    pub(super) fn start(
        mut resolve: impl FnMut(&Value) -> Result<Value, PhaseFailure> + Send + 'static,
    ) -> Result<Self, PhaseFailure> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(failure)?;
        listener.set_nonblocking(true).map_err(failure)?;
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random).map_err(failure)?;
        let token = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let endpoint = format!("{}|{token}", listener.local_addr().map_err(failure)?);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut requests = Vec::new();
            let mut pids = std::collections::BTreeSet::new();
            let mut failed = false;
            let mut retained_bytes = 0;
            while !cancelled.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => return Err(failure(error)),
                };
                let result = (|| {
                    let request = receive(&mut stream)?;
                    if request["token"] != token {
                        return Err(failure("invalid live coverage owner token"));
                    }
                    let pid = request["pid"]
                        .as_u64()
                        .filter(|pid| *pid > 0 && *pid <= u32::MAX.into())
                        .ok_or_else(|| failure("invalid coverage requester PID"))?;
                    if !pids.insert(pid) || requests.len() == MAX_REQUESTS {
                        return Err(failure("duplicate or excessive coverage requester"));
                    }
                    let outcome = resolve(&request);
                    let observation = json!({"pid":pid,"request":request["request"],"environment":request["environment"],
                        "decision":if outcome.is_ok(){"pass"}else{"fail"},"detail":outcome.as_ref().err().map(|error| &error.detail)});
                    retained_bytes += serde_json::to_vec(&observation).map_err(failure)?.len();
                    if retained_bytes > 512 * 1024 {
                        return Err(failure(
                            "coverage request observations exceed their byte budget",
                        ));
                    }
                    requests.push(observation);
                    outcome
                })();
                failed |= result.is_err();
                let response = match result {
                    Ok(value) => json!({"decision":"pass","coverage":value}),
                    Err(error) => json!({"decision":"fail","detail":error.detail}),
                };
                if send(&mut stream, &response).is_err() {
                    failed = true;
                }
            }
            Ok(
                json!({"scope":"live-make-test-coverage-v1","decision":if !failed && !requests.is_empty(){"pass"}else{"fail"},
                "requester_process_count":requests.len(),"requests":requests,"reusable":false}),
            )
        });
        Ok(Self {
            endpoint,
            stop,
            thread: Some(thread),
        })
    }

    /// The endpoint capability is passed to the owned Make process, never a public report.
    pub(super) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Stops accepting work and retains all completed request decisions before returning.
    pub(super) fn finish(mut self) -> Result<Value, PhaseFailure> {
        self.stop.store(true, Ordering::Release);
        self.thread
            .take()
            .expect("owned coverage thread")
            .join()
            .map_err(|_| failure("coverage request owner panicked"))?
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Exchanges one bounded serde JSON document with the live owner, without retry/replay.
pub(super) fn request(endpoint: &str, request: Value) -> Result<Value, PhaseFailure> {
    let (address, token) = endpoint
        .split_once('|')
        .ok_or_else(|| failure("missing live coverage endpoint"))?;
    let address: std::net::SocketAddr = address.parse().map_err(failure)?;
    if !address.ip().is_loopback() || token.len() != 64 {
        return Err(failure("invalid local coverage endpoint"));
    }
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(failure)?;
    let envelope = json!({"token":token,"pid":std::process::id(),"request":request["request"],"environment":request["environment"]});
    send(&mut stream, &envelope)?;
    stream.shutdown(Shutdown::Write).map_err(failure)?;
    let response = receive(&mut stream)?;
    if response["decision"] != "pass" {
        return Err(failure(
            response["detail"]
                .as_str()
                .unwrap_or("coverage owner rejected request"),
        ));
    }
    Ok(response)
}

fn receive(stream: &mut TcpStream) -> Result<Value, PhaseFailure> {
    receive_before(stream, Instant::now() + Duration::from_secs(2))
}

fn receive_before(stream: &mut TcpStream, deadline: Instant) -> Result<Value, PhaseFailure> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| failure("coverage request deadline exceeded"))?;
        stream.set_read_timeout(Some(remaining)).map_err(failure)?;
        let count = match stream.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(failure(error)),
        };
        if count == 0 {
            break;
        }
        if bytes.len() as u64 + count as u64 > MAX_MESSAGE {
            return Err(failure("coverage message exceeds byte budget"));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    serde_json::from_slice(&bytes).map_err(failure)
}

fn send(stream: &mut TcpStream, value: &Value) -> Result<(), PhaseFailure> {
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(failure)?;
    let bytes = serde_json::to_vec(value).map_err(failure)?;
    if bytes.len() as u64 > MAX_MESSAGE {
        return Err(failure("coverage response exceeds byte budget"));
    }
    stream.write_all(&bytes).map_err(failure)
}

#[cfg(test)]
#[path = "coverage_bridge_test.rs"]
mod tests;
