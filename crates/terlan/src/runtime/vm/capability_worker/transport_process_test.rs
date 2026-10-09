//! Closing worker transport must retire helpers that inherit its output pipe.

use super::*;
use std::io::BufRead;
use terlan_process_owner::ProcessControl;

#[test]
fn transport_drop_retires_descendant_pipes() {
    const CHILD: &str = "TERLAN_TEST_CAPABILITY_TRANSPORT_DROP";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = concat!(module_path!(), "::transport_drop_retires_descendant_pipes");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                name.strip_prefix("terlan::").unwrap(),
                "--nocapture",
            ])
            .env(CHILD, "1");
        let output = ProcessControl::new(Duration::from_secs(10))
            .capture_stdout(&mut command, 65_536, |_| Ok(()))
            .expect("transport cleanup must finish while the descendant still holds stdout");
        assert!(String::from_utf8_lossy(&output).contains("1 passed; 0 failed"));
        return;
    }

    let mut command = Command::new("sh");
    command
        .args(["-c", "sleep 30 & printf 'ready\\n'; wait"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = OwnedChild::spawn(command).unwrap();
    let input = child.take_stdin().unwrap();
    let mut output = BufReader::new(child.take_stdout().unwrap());
    let mut ready = String::new();
    output.read_line(&mut ready).unwrap();
    assert_eq!(ready, "ready\n");
    let transport =
        VmCapabilityWorkerTransport::from_streams(input, output, 4_096, 1, Some(child), None)
            .unwrap();
    drop(transport);
}
