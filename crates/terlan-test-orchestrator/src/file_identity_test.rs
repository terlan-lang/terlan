use super::hash_file;
use sha2::Sha256;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

#[test]
fn missing_file_diagnostic_identifies_the_exact_input() {
    let path = PathBuf::from(format!(
        "/missing-terlan-input-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let error = hash_file(
        &path,
        &mut Sha256::default(),
        1024,
        ProcessControl::new(Duration::from_secs(1)),
        Instant::now(),
    )
    .expect_err("missing input must fail identity admission");

    assert_eq!(error.outcome, "input-identity-failed");
    assert!(error.detail.contains(path.to_str().unwrap()));
    assert!(error.detail.contains("No such file"));
}
