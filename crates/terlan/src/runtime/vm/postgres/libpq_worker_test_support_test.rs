use std::{collections::BTreeSet, time::Duration};

use super::*;
use crate::{
    runtime::vm::{postgres::VmPostgresConnectConfig, process::VmProcessId},
    terlan_native::postgres::libpq::DriverReadinessPoller,
};

pub(super) fn request(id: u64, operation: VmPostgresDriverOperation) -> VmPostgresDriverRequest {
    VmPostgresDriverRequest {
        request_id: RequestId { value: id },
        owner: VmProcessId::from_raw_for_test(7),
        operation,
    }
}

pub(super) fn config(url: &str) -> VmPostgresConnectConfig {
    VmPostgresConnectConfig::new(postgres::Config::new(url).with_pool_limits(1, 4))
        .expect("valid Postgres test config")
}

pub(super) fn complete(
    worker: &mut VmPostgresLibpqWorker,
    request: VmPostgresDriverRequest,
) -> VmPostgresDriverCompletion {
    let expected = request.request_id;
    worker.submit(request);
    let poller = DriverReadinessPoller::new().expect("create Postgres test readiness poller");
    let mut ready = BTreeSet::new();
    for _ in 0..10_000 {
        if let Some((request_id, completion)) = worker.drive_socket_ready(&ready) {
            assert_eq!(request_id, expected);
            return completion;
        }
        ready.clear();
        if !worker.waits().is_empty() {
            ready = worker
                .wait_ready(&poller, Some(Duration::from_secs(5)))
                .expect("wait for Postgres test socket readiness");
        }
    }
    panic!("Postgres worker did not complete request {expected:?}");
}
