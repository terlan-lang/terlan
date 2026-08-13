use super::{
    NativeExchangeBroker, NativeExchangePayload, NativeExchangeToken, NativeTensorExchange,
};

fn tensor() -> NativeExchangePayload {
    NativeExchangePayload::Tensor(NativeTensorExchange {
        version: 1,
        device_type: 1,
        device_id: 0,
        dtype_code: 2,
        dtype_bits: 64,
        dtype_lanes: 1,
        shape: vec![2, 2],
        strides: Some(vec![2, 1]),
        byte_offset: 0,
        data: [1.0f64, 2.0, 3.0, 4.0]
            .into_iter()
            .flat_map(f64::to_ne_bytes)
            .collect(),
    })
}

#[test]
fn native_exchange_claim_is_one_shot_and_cleanup_is_exactly_once() {
    let mut broker = NativeExchangeBroker::new();
    let token = broker
        .publish(7, "ndarray", "pytorch", tensor())
        .expect("publish tensor");
    assert_eq!(
        broker
            .claim(token, 7, "pytorch", "tensor.dlpack.v1")
            .expect("claim tensor"),
        tensor()
    );
    assert_eq!(
        broker
            .claim(token, 7, "pytorch", "tensor.dlpack.v1")
            .expect_err("second claim must fail")
            .code(),
        "native_exchange.already_claimed"
    );
    broker.close_claim(token).expect("close claim");
    assert_eq!(
        broker
            .claim(token, 7, "pytorch", "tensor.dlpack.v1")
            .expect_err("closed token must remain one-shot")
            .code(),
        "native_exchange.already_claimed"
    );
    broker.shutdown();
    assert_eq!(broker.cleanup_events(token), 1);
}

#[test]
fn native_exchange_rejects_wrong_kind_owner_consumer_and_forgery_without_claiming() {
    let mut broker = NativeExchangeBroker::new();
    let token = broker
        .publish(11, "polars", "ndarray", tensor())
        .expect("publish tensor");
    for (owner, consumer, kind, code) in [
        (
            12,
            "ndarray",
            "tensor.dlpack.v1",
            "native_exchange.owner_mismatch",
        ),
        (
            11,
            "pytorch",
            "tensor.dlpack.v1",
            "native_exchange.consumer_mismatch",
        ),
        (11, "ndarray", "arrow.c.v1", "native_exchange.kind_mismatch"),
    ] {
        assert_eq!(
            broker
                .claim(token, owner, consumer, kind)
                .expect_err("mismatched claim must fail")
                .code(),
            code
        );
    }
    let (id, generation, authentication) = token.fields();
    let forged = NativeExchangeToken::from_fields(id, generation, authentication ^ 1);
    assert_eq!(
        broker
            .claim(forged, 11, "ndarray", "tensor.dlpack.v1")
            .expect_err("forged token must fail")
            .code(),
        "native_exchange.stale"
    );
    broker
        .claim(token, 11, "ndarray", "tensor.dlpack.v1")
        .expect("failed claims preserve availability");
    broker.close_claim(token).expect("close claim");
}

#[test]
fn native_exchange_cleanup_covers_actor_exit_helper_failure_and_shutdown() {
    let mut broker = NativeExchangeBroker::new();
    let actor_exit = broker
        .publish(1, "ndarray", "pytorch", tensor())
        .expect("actor token");
    let helper_failure = broker
        .publish(2, "polars", "ndarray", tensor())
        .expect("helper token");
    let shutdown = broker
        .publish(3, "ndarray", "pytorch", tensor())
        .expect("shutdown token");
    let claimed_failure = broker
        .publish(4, "ndarray", "pytorch", tensor())
        .expect("claimed failure token");
    broker
        .claim(claimed_failure, 4, "pytorch", "tensor.dlpack.v1")
        .expect("claim before actor failure");

    broker.close_owner(1);
    broker.close_producer("polars");
    broker.close_owner(4);
    broker.shutdown();
    broker.close_owner(1);

    for token in [actor_exit, helper_failure, shutdown, claimed_failure] {
        assert_eq!(broker.cleanup_events(token), 1);
    }
}

#[test]
fn native_exchange_validates_tensor_metadata_before_publication() {
    let cases = [
        (
            {
                let mut value = tensor();
                let NativeExchangePayload::Tensor(tensor) = &mut value;
                tensor.version = 0;
                value
            },
            "native_exchange.tensor.version",
        ),
        (
            {
                let mut value = tensor();
                let NativeExchangePayload::Tensor(tensor) = &mut value;
                tensor.device_type = 2;
                value
            },
            "native_exchange.tensor.device",
        ),
        (
            {
                let mut value = tensor();
                let NativeExchangePayload::Tensor(tensor) = &mut value;
                tensor.strides = Some(vec![1, 2]);
                value
            },
            "native_exchange.tensor.layout",
        ),
        (
            {
                let mut value = tensor();
                let NativeExchangePayload::Tensor(tensor) = &mut value;
                tensor.data.pop();
                value
            },
            "native_exchange.tensor.byte_count",
        ),
    ];

    for (payload, code) in cases {
        let mut broker = NativeExchangeBroker::new();
        let error = broker
            .publish(1, "ndarray", "pytorch", payload)
            .expect_err("malformed tensor must fail");
        assert_eq!(error.code(), code);
        assert!(!error.message().is_empty());
    }
}

#[test]
fn native_exchange_admits_cross_package_scalar_packet_types() {
    for (dtype_code, dtype_bits, element_bytes) in [
        (6, 8, 1usize),
        (1, 8, 1usize),
        (0, 32, 4usize),
        (0, 64, 8usize),
        (2, 32, 4usize),
        (2, 64, 8usize),
    ] {
        let mut payload = tensor();
        let NativeExchangePayload::Tensor(tensor) = &mut payload;
        tensor.dtype_code = dtype_code;
        tensor.dtype_bits = dtype_bits;
        tensor.data = vec![0; 4 * element_bytes];
        let mut broker = NativeExchangeBroker::new();
        let token = broker
            .publish(1, "cuda", "opencv", payload)
            .expect("supported scalar packet must publish");
        broker
            .claim(token, 1, "opencv", "tensor.dlpack.v1")
            .expect("supported scalar packet must be claimable");
    }
}

#[test]
fn native_exchange_tensor_packet_round_trip_hides_payload_behind_token() {
    let mut broker = NativeExchangeBroker::new();
    let packet = super::encode_tensor_packet(match &tensor() {
        NativeExchangePayload::Tensor(value) => value,
    });
    let token = broker
        .publish_tensor_packet(5, "ndarray", "pytorch", &packet)
        .expect("publish packet");
    assert_eq!(&token[..4], b"TNXT");
    assert_ne!(token, packet);
    let (claimed, decoded) = broker
        .claim_tensor_packet(&token, 5, "pytorch")
        .expect("claim packet")
        .expect("exchange token");
    assert_eq!(decoded, packet);
    broker.close_claim(claimed).expect("close packet claim");
    assert_eq!(broker.cleanup_events(claimed), 1);
}
