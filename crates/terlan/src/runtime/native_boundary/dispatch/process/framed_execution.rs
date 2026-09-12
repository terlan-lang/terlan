//! Execution coordination for phased framed child protocols.

use super::framed::FramedProcessRequest;
use super::*;

pub(super) fn execute_length_framed(
    request: FramedProcessRequest,
    cancellation: Option<&NativeBoundaryCancellationToken>,
) -> Result<NativeBoundaryValue, DispatchError> {
    let started = Instant::now();
    let pipes = PipeScope::new(started, request.command.timeout, cancellation);
    if let Some((code, message)) = pipes.failure() {
        return Ok(process_error(code, message, request.command.program));
    }
    let command = configured_command(&request.command);
    let mut child = match OwnedChild::spawn(command) {
        Ok(child) => child,
        Err(error) => {
            return Ok(process_error(
                "spawn_failed",
                format!("cannot spawn child: {error}"),
                request.command.program,
            ))
        }
    };

    let input = child
        .take_stdin()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stdin pipe is unavailable", 0))?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stdout pipe is unavailable", 0))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| DispatchError::new("process.pipe", "child stderr pipe is unavailable", 0))?;
    let mut input = pipes.wrap(input).map_err(pipe_setup_error)?;
    let stdout = pipes.wrap(stdout).map_err(pipe_setup_error)?;
    let stderr = pipes.wrap(stderr).map_err(pipe_setup_error)?;
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

    // A child that never reads stdin must not block the deadline owner.
    // The protocol thread still sequences each write and its response frames.
    let (protocol_sender, protocol_receiver) = mpsc::sync_channel(1);
    let protocol_overflow = Arc::clone(&overflow);
    let protocol_cancellation = cancellation.cloned();
    let timeout = request.command.timeout;
    let protocol_thread = std::thread::spawn(move || {
        let outcome = (|| {
            write_framed_input(&mut input, &request.command.stdin)?;
            for exchange in request.exchanges {
                write_framed_input(&mut input, &exchange.input)?;
                for _ in 0..exchange.expected_frames {
                    await_frame(
                        &frame_receiver,
                        started,
                        timeout,
                        &protocol_overflow,
                        protocol_cancellation.as_ref(),
                    )?;
                }
            }
            Ok(())
        })();
        drop(input);
        let _ = protocol_sender.send(outcome);
        // Keep the notification receiver alive until stdout has been drained.
        // Dropping it here would truncate frames arriving after the final phase.
        frame_receiver
    });
    let failure = await_protocol(
        &protocol_receiver,
        started,
        timeout,
        &overflow,
        cancellation,
    )
    .err();

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
            let (code, message) = pipes.failure().unwrap_or((code, message));
            pipes.stop();
            child.finish().map_err(|error| {
                DispatchError::new(
                    "process.cleanup",
                    format!("cannot terminate child: {error}"),
                    0,
                )
            })?;
            let _ = protocol_thread.join();
            let _ = join_framed_output(stdout_thread);
            let _ = join_output(stderr_thread);
            return Ok(process_error(code, message, request.command.program));
        }
        ProcessCompletion::Status(status) => status,
    };
    let drain_failure = pipes.drain(&overflow, || {
        protocol_thread.is_finished() && stdout_thread.is_finished() && stderr_thread.is_finished()
    });
    let _frame_receiver = protocol_thread.join();
    let frames = join_framed_output(stdout_thread);
    let stderr = join_output(stderr_thread);
    if let Some((code, message)) = drain_failure.or_else(|| pipes.failure()) {
        return Ok(process_error(code, message, request.command.program));
    }
    let frames = match frames {
        Ok(frames) => frames,
        Err(message) => {
            return Ok(process_error(
                "invalid_frame",
                message.to_string(),
                request.command.program,
            ));
        }
    };
    let stderr = stderr?;
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

fn await_protocol(
    receiver: &mpsc::Receiver<Result<(), (&'static str, String)>>,
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
        match receiver.recv_timeout(remaining.min(Duration::from_millis(2))) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err((
                    "invalid_frame",
                    "child protocol coordinator panicked".to_string(),
                ));
            }
        }
    }
}
