use super::*;

#[test]
fn live_owner_answers_once_and_cannot_be_reused_after_shutdown() {
    let bridge = Bridge::start(|request| Ok(request["request"].clone())).unwrap();
    let endpoint = bridge.endpoint().to_owned();
    assert_eq!(
        request(
            &endpoint,
            json!({"request":{"id":"one"},"environment":"same"})
        )
        .unwrap()["decision"],
        "pass"
    );
    let report = bridge.finish().unwrap();
    assert_eq!(report["decision"], "pass");
    assert_eq!(report["requester_process_count"], 1);
    assert!(!report
        .to_string()
        .contains(endpoint.split_once('|').unwrap().1));
    assert!(request(&endpoint, json!({"request":{}})).is_err());
}

#[test]
fn failed_duplicate_and_unauthenticated_requests_prevent_success() {
    for mode in ["failed", "duplicate", "token"] {
        let bridge = Bridge::start(move |_| {
            if mode == "failed" {
                Err(failure("uncovered"))
            } else {
                Ok(json!({}))
            }
        })
        .unwrap();
        let endpoint = bridge.endpoint().to_owned();
        let message = json!({"request":{},"environment":"same"});
        if mode == "duplicate" {
            request(&endpoint, message.clone()).unwrap();
            assert!(request(&endpoint, message).is_err());
        } else if mode == "token" {
            let address = endpoint.split_once('|').unwrap().0;
            assert!(request(&format!("{address}|{}", "0".repeat(64)), message).is_err());
        } else {
            assert!(request(&endpoint, message).is_err());
        }
        assert_eq!(bridge.finish().unwrap()["decision"], "fail");
    }
}

#[test]
fn idle_owner_is_not_success_and_drop_closes_its_endpoint() {
    let bridge = Bridge::start(|_| Ok(json!({}))).unwrap();
    assert_eq!(bridge.finish().unwrap()["decision"], "fail");
    let bridge = Bridge::start(|_| Ok(json!({}))).unwrap();
    let endpoint = bridge.endpoint().to_owned();
    drop(bridge);
    assert!(request(&endpoint, json!({"request":{}})).is_err());
}

#[test]
fn incomplete_messages_obey_one_total_deadline() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    client.write_all(b"{").unwrap();
    let started = Instant::now();
    assert!(receive_before(&mut server, started + Duration::from_millis(40)).is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    client.write_all(b" ").unwrap();
    assert!(receive_before(&mut server, started)
        .unwrap_err()
        .detail
        .contains("deadline"));
}
