//! Behavioral coverage of request-owned processes and inherited pipes.

use super::*;

fn request(script: &str) -> ProcessRequest {
    ProcessRequest {
        program: "/bin/sh".to_string(),
        arguments: vec!["-c".to_string(), script.to_string()],
        working_directory: None,
        environment: vec![],
        removed_environment: vec![],
        stdin: vec![],
        timeout: Duration::from_millis(150),
        output_limit: 4096,
    }
}

fn code(value: &NativeBoundaryValue) -> &str {
    let NativeBoundaryValue::Record { name, fields } = value else {
        panic!("result")
    };
    if name == "Ok" {
        return "ok";
    }
    let NativeBoundaryValue::Record { fields, .. } = &fields[0].1 else {
        panic!("error")
    };
    let NativeBoundaryValue::Atom(code) = field(fields, "code").unwrap() else {
        panic!("code")
    };
    code
}

#[test]
fn exited_leader_cannot_leave_descendants_holding_output_pipes() {
    let started = Instant::now();
    let result = execute(request("sleep 4 & printf complete; exit 7"), None).unwrap();
    assert_eq!(code(&result), "ok");
    assert!(started.elapsed() < Duration::from_secs(2));
    let NativeBoundaryValue::Record { fields, .. } = result else {
        panic!("result")
    };
    let NativeBoundaryValue::Record { fields, .. } = &fields[0].1 else {
        panic!("output")
    };
    assert_eq!(
        field(fields, "status").unwrap(),
        &NativeBoundaryValue::Int(7)
    );
    assert_eq!(
        field(fields, "stdout").unwrap(),
        &NativeBoundaryValue::Text("complete".into())
    );
}

#[test]
fn timeout_closes_descendant_pipes_and_blocked_input() {
    let mut command = request("sleep 4 & wait");
    command.stdin = vec![b'x'; MAX_STDIN_BYTES];
    let started = Instant::now();
    assert_eq!(code(&execute(command, None).unwrap()), "timed_out");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn internal_capture_preserves_non_utf8_and_nul_bytes() {
    let output = capture(
        request("printf '\\377\\000a'; printf '\\376\\000b' >&2"),
        None,
    )
    .unwrap();
    assert_eq!(output.stdout, vec![255, 0, b'a']);
    assert_eq!(output.stderr, vec![254, 0, b'b']);
}

#[test]
fn output_limit_terminates_the_group() {
    let mut command = request("sleep 4 & while :; do printf output; done");
    command.timeout = Duration::from_secs(3);
    let started = Instant::now();
    assert_eq!(
        code(&execute(command, None).unwrap()),
        "output_limit_exceeded"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn framed_exited_leader_cannot_leave_descendant_pipes() {
    let request = framed::FramedProcessRequest {
        command: request("sleep 4 & printf 'Content-Length: 2\\r\\n\\r\\nok'"),
        exchanges: vec![],
        length_header: "Content-Length".to_string(),
    };
    let started = Instant::now();
    let result = framed_execution::execute_length_framed(request, None).unwrap();
    assert_eq!(code(&result), "ok");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(target_os = "linux")]
#[test]
fn cleanup_terminates_a_live_descendant_not_just_its_pipes() {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "sleep 4 & echo $!; wait"])
        .stdout(Stdio::piped());
    let mut child = OwnedChild::spawn(command).unwrap();
    let mut reader = BufReader::new(child.take_stdout().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let pid: u32 = line.trim().parse().unwrap();
    child.finish().unwrap();
    let started = Instant::now();
    loop {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            // Linux can remove the task between opening and reading procfs.
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) =>
            {
                break
            }
            Ok(stat)
                if stat
                    .rsplit_once(')')
                    .unwrap()
                    .1
                    .trim_start()
                    .starts_with('Z') =>
            {
                break
            }
            Ok(_) => {}
            Err(error) => panic!("inspect descendant: {error}"),
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "descendant still running"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn cancellation_closes_descendant_pipes() {
    let cancellation = NativeBoundaryCancellationToken::new();
    let signal = cancellation.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        signal.cancel();
    });
    let started = Instant::now();
    let mut command = request("sleep 4 & wait");
    command.timeout = Duration::from_secs(3);
    let result = execute(command, Some(&cancellation)).unwrap();
    canceller.join().unwrap();
    assert_eq!(code(&result), "cancelled");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn framed_input_that_is_never_read_still_times_out() {
    let command = request("sleep 4 & wait");
    let request = framed::FramedProcessRequest {
        command,
        exchanges: vec![framed::FramedExchangeRequest {
            input: vec![b'x'; MAX_STDIN_BYTES],
            expected_frames: 1,
        }],
        length_header: "Content-Length".to_string(),
    };
    let started = Instant::now();
    let result = framed_execution::execute_length_framed(request, None).unwrap();
    assert_eq!(code(&result), "timed_out");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn unwinding_owner_reaps_child_without_killing_another_owner() {
    let mut other_command = Command::new("/bin/sh");
    other_command.args(["-c", "sleep 4"]);
    let mut other = OwnedChild::spawn(other_command).unwrap();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 4"]);
    let child = OwnedChild::spawn(command).unwrap();
    let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
    let result = std::panic::catch_unwind(move || {
        let _owner = child;
        panic!("injected owner panic");
    });
    assert!(result.is_err());
    assert_eq!(
        rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG).unwrap_err(),
        rustix::io::Errno::CHILD
    );
    assert!(other.try_wait().unwrap().is_none());
    other.finish().unwrap();
}
