//! Pipe deadlines must not depend on terminating the process holding the peer.

use super::*;

fn holder() -> OwnedChild {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "sleep 4"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    OwnedChild::spawn(command).unwrap()
}

#[test]
fn output_deadline_does_not_require_peer_process_exit() {
    let mut peer = holder();
    let started = Instant::now();
    let scope = PipeScope::new(started, Duration::from_millis(40), None);
    let mut pipe = scope.wrap(peer.take_stdout().unwrap()).unwrap();
    let error = pipe.read_to_end(&mut vec![]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        peer.try_wait().unwrap().is_none(),
        "unrelated peer must remain alive"
    );
}

#[test]
fn input_deadline_does_not_require_peer_to_read_or_exit() {
    let mut peer = holder();
    let started = Instant::now();
    let scope = PipeScope::new(started, Duration::from_millis(40), None);
    let mut pipe = scope.wrap(peer.take_stdin().unwrap()).unwrap();
    let error = pipe.write_all(&vec![b'x'; MAX_STDIN_BYTES]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(peer.try_wait().unwrap().is_none());
}

#[test]
fn scope_unwind_interrupts_a_reader_without_killing_its_peer() {
    let mut peer = holder();
    let started = Instant::now();
    let scope = PipeScope::new(started, Duration::from_secs(3), None);
    let mut pipe = scope.wrap(peer.take_stdout().unwrap()).unwrap();
    let reader = std::thread::spawn(move || pipe.read_to_end(&mut vec![]));
    let outcome = std::panic::catch_unwind(move || {
        let _scope = scope;
        panic!("injected pipe owner panic");
    });
    assert!(outcome.is_err());
    assert_eq!(
        reader.join().unwrap().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(peer.try_wait().unwrap().is_none());
}

#[test]
fn cancellation_interrupts_a_reader_without_killing_its_peer() {
    let mut peer = holder();
    let token = NativeBoundaryCancellationToken::new();
    let started = Instant::now();
    let scope = PipeScope::new(started, Duration::from_secs(3), Some(&token));
    let mut pipe = scope.wrap(peer.take_stderr().unwrap()).unwrap();
    let reader = std::thread::spawn(move || pipe.read_to_end(&mut vec![]));
    token.cancel();
    assert_eq!(
        reader.join().unwrap().unwrap_err().kind(),
        std::io::ErrorKind::Other
    );
    assert_eq!(scope.failure().unwrap().0, "cancelled");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(peer.try_wait().unwrap().is_none());
}

#[test]
fn already_cancelled_requests_do_not_attempt_a_spawn() {
    let token = NativeBoundaryCancellationToken::new();
    token.cancel();
    let request = ProcessRequest {
        program: "/__terlan_no_such_program__".into(),
        arguments: vec![],
        working_directory: None,
        environment: vec![],
        removed_environment: vec![],
        stdin: vec![],
        timeout: Duration::from_secs(1),
        output_limit: 1024,
    };
    assert!(matches!(
        capture(request, Some(&token)),
        Err(CaptureFailure::Process("cancelled", _))
    ));
}

#[test]
fn overflow_during_drain_interrupts_an_idle_pipe_worker() {
    let mut peer = holder();
    let started = Instant::now();
    let scope = PipeScope::new(started, Duration::from_secs(3), None);
    let mut pipe = scope.wrap(peer.take_stderr().unwrap()).unwrap();
    let reader = std::thread::spawn(move || pipe.read_to_end(&mut vec![]));
    let overflow = AtomicBool::new(true);
    assert_eq!(
        scope.drain(&overflow, || reader.is_finished()).unwrap().0,
        "output_limit_exceeded"
    );
    assert_eq!(
        reader.join().unwrap().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(peer.try_wait().unwrap().is_none());
}
