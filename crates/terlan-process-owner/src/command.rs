//! Closed-stdin tool execution with a shared execution/pipe deadline.

use std::io::{self, Read};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::OwnedChild;

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;
const OUTPUT_QUEUE_CHUNKS: usize = 64;

/// Per-operation deadline and optional caller-owned cancellation signal.
#[derive(Clone, Copy)]
pub struct ProcessControl<'a> {
    timeout: Duration,
    cancellation: Option<&'a AtomicBool>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    enclosing_process_group: Option<u32>,
}

impl<'a> ProcessControl<'a> {
    /// Creates execution control without installing global signal handlers.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            cancellation: None,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            enclosing_process_group: None,
        }
    }

    /// Shares the caller's cancellation flag across execution and pipe drainage.
    pub fn with_cancellation(mut self, cancellation: &'a AtomicBool) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    /// Caps a cheap preflight without extending its caller's deadline or losing cancellation.
    pub fn limited_to(mut self, maximum: Duration) -> Self {
        self.timeout = self.timeout.min(maximum);
        self
    }

    /// Keeps nested launches in the caller's enclosing Linux/macOS owner group.
    ///
    /// Membership is checked before each spawn. The enclosing owner must retain
    /// its leader and perform whole-group cleanup; inner timeout, cancellation,
    /// or completion owns only the direct child, never its siblings. This is not
    /// containment of children which deliberately escape with setpgid/setsid.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn with_enclosing_process_group(mut self, owner: u32) -> Self {
        self.enclosing_process_group = Some(owner);
        self
    }

    fn spawn(self, command: &mut Command) -> Result<OwnedChild, Failure> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Some(owner) = self.enclosing_process_group {
            return OwnedChild::spawn_in_enclosing_group(command, owner)
                .map_err(|error| Failure::new("launch-failed", error));
        }
        OwnedChild::spawn_command(command).map_err(|error| Failure::new("launch-failed", error))
    }

    /// Runs with inherited output and observed, cancellable child ownership.
    pub fn run(
        self,
        command: &mut Command,
        launched: impl FnOnce(u32) -> Result<(), String>,
    ) -> Result<(), Failure> {
        run_controlled(command, self, launched)
    }

    /// Captures bounded stdout with the same cancellation and execution deadline.
    pub fn capture_stdout(
        self,
        command: &mut Command,
        limit: usize,
        launched: impl FnOnce(u32) -> Result<(), String>,
    ) -> Result<Vec<u8>, Failure> {
        let captured = capture_controlled(command, self, limit, false, launched, |_| Ok(()))?;
        captured.outcome?;
        Ok(captured.stdout)
    }

    /// Retains bounded stdout after a nonzero exit so callers can report test diagnostics.
    /// Launch, capture, timeout, and cancellation failures remain outer errors.
    pub fn capture_stdout_result(
        self,
        command: &mut Command,
        limit: usize,
        launched: impl FnOnce(u32) -> Result<(), String>,
    ) -> Result<CapturedOutput, Failure> {
        capture_controlled(command, self, limit, true, launched, |_| Ok(()))
    }

    /// Observes bounded stdout chunks on the owner thread before process completion.
    ///
    /// Concatenating successful observations yields exactly the returned stdout.
    /// Chunk boundaries have no record semantics. The callback must be short and
    /// nonblocking; callback failure or panic retains child and reader cleanup.
    /// Observations are provisional until the capture and exit outcome succeed.
    pub fn capture_stdout_observed(
        self,
        command: &mut Command,
        limit: usize,
        launched: impl FnOnce(u32) -> Result<(), String>,
        observed: impl FnMut(&[u8]) -> Result<(), String>,
    ) -> Result<CapturedOutput, Failure> {
        capture_controlled(command, self, limit, true, launched, observed)
    }

    /// Checks the same deadline/cancellation during caller-owned preflight work.
    pub fn check(self, started: Instant) -> Result<(), Failure> {
        if self
            .cancellation
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return Err(Failure::new("cancelled", "process owner was cancelled"));
        }
        check_deadline(started, self.timeout)
    }
}

/// Bounded captured bytes and the separately checked direct-child exit outcome.
#[derive(Debug)]
pub struct CapturedOutput {
    /// Byte-exact stdout, including diagnostics from a failed child.
    pub stdout: Vec<u8>,
    /// The observed exit status; captured output does not imply successful execution.
    pub outcome: Result<(), Failure>,
}

/// An attributed process failure suitable for a producer's outcome ledger.
#[derive(Debug)]
pub struct Failure {
    /// Stable outcome category, including timeout, exit failure, and overflow.
    pub kind: &'static str,
    /// Diagnostic detail retaining the underlying operating-system error.
    pub detail: String,
}

impl Failure {
    fn new(kind: &'static str, detail: impl ToString) -> Self {
        Self {
            kind,
            detail: detail.to_string(),
        }
    }
}

/// Runs a noninteractive tool with inherited output and owned child cleanup.
pub fn run(command: &mut Command, timeout: Duration) -> Result<(), Failure> {
    run_with_launch(command, timeout, |_| Ok(()))
}

/// Observes a real spawn before continuing; observation failure cleans the child.
pub fn run_with_launch(
    command: &mut Command,
    timeout: Duration,
    launched: impl FnOnce(u32) -> Result<(), String>,
) -> Result<(), Failure> {
    ProcessControl::new(timeout).run(command, launched)
}

fn run_controlled(
    command: &mut Command,
    control: ProcessControl<'_>,
    launched: impl FnOnce(u32) -> Result<(), String>,
) -> Result<(), Failure> {
    validate_limits(control.timeout, 1)?;
    let started = Instant::now();
    control.check(started)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = control.spawn(command)?;
    launched(child.id()).map_err(|error| Failure::new("observation-failed", error))?;
    loop {
        control.check(started)?;
        if let Some(status) = child
            .try_wait()
            .map_err(|error| Failure::new("wait-failed", error))?
        {
            return require_success(status);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Captures byte-exact stdout up to `limit`; stderr remains streamed to the user.
///
/// Execution and drainage share one deadline. Linux/macOS use interruptible
/// nonblocking pipes. On other hosts a foreign descendant retaining the pipe
/// may outlive the reader thread, but cannot make the caller join indefinitely;
/// complete Windows descendant/handle cleanup still requires job ownership.
pub fn capture_stdout(
    command: &mut Command,
    timeout: Duration,
    limit: usize,
) -> Result<Vec<u8>, Failure> {
    capture_stdout_with_launch(command, timeout, limit, |_| Ok(()))
}

/// Captures bounded stdout while recording the actual spawn under child ownership.
pub fn capture_stdout_with_launch(
    command: &mut Command,
    timeout: Duration,
    limit: usize,
    launched: impl FnOnce(u32) -> Result<(), String>,
) -> Result<Vec<u8>, Failure> {
    ProcessControl::new(timeout).capture_stdout(command, limit, launched)
}

fn capture_controlled(
    command: &mut Command,
    control: ProcessControl<'_>,
    limit: usize,
    retain_failed_output: bool,
    launched: impl FnOnce(u32) -> Result<(), String>,
    mut observed: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<CapturedOutput, Failure> {
    validate_limits(control.timeout, limit)?;
    let started = Instant::now();
    control.check(started)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = control.spawn(command)?;
    launched(child.id()).map_err(|error| Failure::new("observation-failed", error))?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| Failure::new("capture-failed", "missing stdout"))?;
    let reader = Reader::new(stdout, limit)?;
    let mut output = Vec::new();
    let mut drained = false;
    let mut exited = None;
    loop {
        control.check(started)?;
        // Drain a bounded batch without imposing one polling delay per 8 KiB.
        for _ in 0..OUTPUT_QUEUE_CHUNKS {
            if drained {
                break;
            }
            control.check(started)?;
            match reader.event()? {
                Some(ReadEvent::Chunk(bytes)) => {
                    observed(&bytes)
                        .map_err(|error| Failure::new("output-observation-failed", error))?;
                    output.extend_from_slice(&bytes);
                }
                Some(ReadEvent::Finished(result)) => {
                    result?;
                    drained = true;
                }
                None => break,
            }
        }
        if exited.is_none() {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| Failure::new("wait-failed", error))?
            {
                if !retain_failed_output {
                    require_success(status)?;
                }
                exited = Some(status);
            }
        }
        if let Some(status) = exited {
            if drained {
                return Ok(CapturedOutput {
                    stdout: output,
                    outcome: require_success(status),
                });
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn validate_limits(timeout: Duration, limit: usize) -> Result<(), Failure> {
    if timeout.is_zero() || limit == 0 || limit > MAX_CAPTURE_BYTES {
        return Err(Failure::new(
            "invalid-limits",
            "positive timeout and 1..=64 MiB capture limit required",
        ));
    }
    Ok(())
}

fn check_deadline(started: Instant, timeout: Duration) -> Result<(), Failure> {
    if started.elapsed() >= timeout {
        return Err(Failure::new(
            "timed-out",
            format!(
                "execution or pipe drainage exceeded {}ms",
                timeout.as_millis()
            ),
        ));
    }
    Ok(())
}

fn require_success(status: std::process::ExitStatus) -> Result<(), Failure> {
    if status.success() {
        Ok(())
    } else {
        Err(Failure::new(
            "failed",
            format!("child exited with status {status}"),
        ))
    }
}

struct Reader {
    receiver: mpsc::Receiver<ReadEvent>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

enum ReadEvent {
    Chunk(Vec<u8>),
    Finished(Result<(), Failure>),
}

impl Reader {
    fn new(stdout: ChildStdout, limit: usize) -> Result<Self, Failure> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use rustix::fs::{fcntl_getfl, fcntl_setfl, OFlags};
            fcntl_setfl(
                &stdout,
                fcntl_getfl(&stdout).map_err(|error| Failure::new("capture-failed", error))?
                    | OFlags::NONBLOCK,
            )
            .map_err(|error| Failure::new("capture-failed", error))?;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        // At most 512 KiB plus one pending chunk, within the overall byte limit.
        let (sender, receiver) = mpsc::sync_channel(OUTPUT_QUEUE_CHUNKS);
        let thread = thread::Builder::new()
            .name("tool-stdout".into())
            .spawn(move || {
                let result = read_bounded(stdout, limit, &stopped, &sender);
                send_event(&sender, ReadEvent::Finished(result), &stopped);
            })
            .map_err(|error| Failure::new("capture-failed", error))?;
        Ok(Self {
            receiver,
            stop,
            thread: Some(thread),
        })
    }

    fn event(&self) -> Result<Option<ReadEvent>, Failure> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Failure::new(
                "capture-failed",
                "stdout reader stopped without a result",
            )),
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            if cfg!(any(target_os = "linux", target_os = "macos")) || thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

fn read_bounded(
    mut stdout: ChildStdout,
    limit: usize,
    stop: &AtomicBool,
    sender: &mpsc::SyncSender<ReadEvent>,
) -> Result<(), Failure> {
    let mut bytes = 0;
    let mut buffer = [0; 8192];
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(Failure::new("cancelled", "stdout owner stopped"));
        }
        match stdout.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => {
                if count > limit.saturating_sub(bytes) {
                    return Err(Failure::new(
                        "output-limit",
                        "stdout exceeded capture byte limit",
                    ));
                }
                bytes += count;
                if !send_event(sender, ReadEvent::Chunk(buffer[..count].to_vec()), stop) {
                    return Err(Failure::new("cancelled", "stdout owner stopped"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL_INTERVAL),
            Err(error) => return Err(Failure::new("capture-failed", error)),
        }
    }
}

// Never block a reader on a full channel while its owner is unwinding or failing.
fn send_event(
    sender: &mpsc::SyncSender<ReadEvent>,
    mut event: ReadEvent,
    stop: &AtomicBool,
) -> bool {
    loop {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
            Err(mpsc::TrySendError::Full(pending)) => {
                event = pending;
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

#[cfg(test)]
#[path = "command_test.rs"]
mod tests;
