use super::*;

#[test]
fn buffered_body_retains_metadata_length_and_allocation() {
    let bytes = Bytes::from_static(b"buffered");
    let pointer = bytes.as_ptr();
    let mut response = http::Response::builder()
        .status(202)
        .header("x-test", "retained")
        .body(bytes)
        .unwrap();
    response.extensions_mut().insert(42_u64);
    let response = ResponseBody::from_response(response);
    assert_eq!(response.status(), 202);
    assert_eq!(response.headers()["x-test"], "retained");
    assert_eq!(response.extensions().get::<u64>(), Some(&42));
    let mut body = response.into_body();
    assert_eq!(body.size_hint().exact(), Some(8));
    assert!(!body.is_end_stream());
    let mut context = Context::from_waker(std::task::Waker::noop());
    let Poll::Ready(Some(Ok(frame))) = Pin::new(&mut body).poll_frame(&mut context) else {
        panic!("buffered body should be immediately available");
    };
    let data = frame.into_data().unwrap();
    assert_eq!(data.as_ptr(), pointer);
    assert_eq!(data, "buffered");
    assert!(body.is_end_stream());
    assert_eq!(body.size_hint().exact(), Some(0));
    assert!(matches!(
        Pin::new(&mut body).poll_frame(&mut context),
        Poll::Ready(None)
    ));
}

#[test]
fn stream_polling_preserves_bytes_and_bounds_until_completion() {
    let source = Bytes::from_static("helloé🙂".as_bytes());
    let start = source.as_ptr();
    let stream = HttpResponseChunks::new(vec![Bytes::new(), source], 3, 1).unwrap();
    let mut response = http::Response::new(Bytes::new());
    response.extensions_mut().insert(stream);
    let mut response = ResponseBody::from_response(response);
    assert!(response.extensions().get::<HttpResponseChunks>().is_none());
    let body = response.body_mut();
    assert_eq!(body.size_hint().exact(), None);
    let mut context = Context::from_waker(std::task::Waker::noop());
    let mut output = Vec::new();
    while !body.is_end_stream() {
        let Poll::Ready(Some(Ok(frame))) = Pin::new(&mut *body).poll_frame(&mut context) else {
            panic!("finite stream should emit a frame");
        };
        let chunk = frame.into_data().unwrap();
        assert!(!chunk.is_empty() && chunk.len() <= 3);
        if output.is_empty() {
            assert_eq!(
                chunk.as_ptr(),
                start,
                "first chunk must retain its allocation"
            );
        }
        output.extend_from_slice(&chunk);
    }
    assert_eq!(output, "helloé🙂".as_bytes());
    assert!(matches!(
        Pin::new(body).poll_frame(&mut context),
        Poll::Ready(None)
    ));
}

#[test]
fn stream_rejects_invalid_limits_and_accepts_empty_bodies() {
    for (size, pending) in [(0, 1), (-1, 1), (1, 0), (1, -1)] {
        assert!(HttpResponseChunks::new(vec![], size, pending).is_err());
    }
    let mut stream = HttpResponseChunks::new(vec![Bytes::new()], 1, 1).unwrap();
    assert!(stream.is_complete());
    assert_eq!(stream.next_chunk(), None);
}

#[test]
fn cancelling_a_stream_releases_unemitted_source_chunks() {
    let backing = bytes::Bytes::from(vec![1; 1024]);
    let mut stream = HttpResponseChunks::new(vec![backing.clone()], 1, 1).unwrap();
    drop(stream.next_chunk());
    assert!(!backing.is_unique());
    drop(stream);
    assert!(backing.is_unique());
}
