use super::*;

#[test]
fn empty_resolution_is_reported_without_attempting_a_bind() {
    let error = bind_addresses("empty", 80, [], |_| panic!("no address to bind")).unwrap_err();
    assert_eq!(
        error,
        "error[vm.protocol_bind]: bind empty:80: host resolved to no addresses"
    );
}

#[test]
fn bind_failures_preserve_resolution_order_and_report_the_last_error() {
    let addresses = ["127.0.0.1:80".parse().unwrap(), "[::1]:80".parse().unwrap()];
    let mut attempted = Vec::new();
    let error = bind_addresses("example", 80, addresses, |address| {
        attempted.push(address);
        Err(io::Error::other(format!("failed {address}")))
    })
    .unwrap_err();
    assert_eq!(attempted, addresses);
    assert_eq!(
        error,
        "error[vm.protocol_bind]: bind example:80: failed [::1]:80"
    );
}
