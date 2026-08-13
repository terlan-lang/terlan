use super::*;

#[test]
fn http2_limits_bound_streams_flow_headers_frames_and_owner_tasks() {
    assert_eq!(MAX_CONCURRENT_STREAMS, 256);
    assert_eq!(MAX_PENDING_RESET_STREAMS, 64);
    assert!(INITIAL_STREAM_WINDOW_BYTES < INITIAL_CONNECTION_WINDOW_BYTES);
    assert_eq!(MAX_FRAME_BYTES, 16 * 1024);
    assert_eq!(MAX_HEADER_LIST_BYTES, 64 * 1024);
    assert_eq!(MAX_SEND_BUFFER_BYTES, 1024 * 1024);
    assert!(MAX_OWNER_LOCAL_HTTP2_TASKS >= MAX_CONCURRENT_STREAMS as usize);
}

#[test]
fn owner_local_http2_executor_fails_loudly_at_capacity() {
    let tasks = Rc::new(RefCell::new(Vec::new()));
    let overflowed = Rc::new(Cell::new(false));
    let executor = VmHttp2Executor {
        tasks: Rc::clone(&tasks),
        overflowed: Rc::clone(&overflowed),
    };
    for _ in 0..=MAX_OWNER_LOCAL_HTTP2_TASKS {
        executor.execute(async {});
    }
    assert_eq!(tasks.borrow().len(), MAX_OWNER_LOCAL_HTTP2_TASKS);
    assert!(overflowed.get());
}
