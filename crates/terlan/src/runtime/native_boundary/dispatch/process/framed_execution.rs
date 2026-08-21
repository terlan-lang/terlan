//! Execution coordination for phased framed child protocols.

use super::framed::FramedProcessRequest;
use super::*;

pub(super) fn execute_length_framed(
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
