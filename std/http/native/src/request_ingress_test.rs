use super::*;
use std::future::Future;
use std::io;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use futures_util::{stream, StreamExt};
use http_body_util::{Full, StreamBody};
use hyper::body::Frame;

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("in-memory ingress unexpectedly suspended"),
    }
}

fn text(bytes: &'static [u8]) -> Request<Full<Bytes>> {
    Request::new(Full::new(Bytes::from_static(bytes)))
}

fn frames(
    items: Vec<Result<Frame<Bytes>, io::Error>>,
) -> Request<impl Body<Data = Bytes, Error = io::Error> + Unpin> {
    Request::new(StreamBody::new(stream::iter(items)))
}

#[test]
fn text_admission_preserves_metadata_and_multiframe_utf8_without_creating_files() {
    let mut request = frames(vec![
        Ok(Frame::data(Bytes::from_static(b"\xe2"))),
        Ok(Frame::data(Bytes::from_static(b"\x82\xac"))),
        Ok(Frame::trailers(http::HeaderMap::new())),
    ]);
    *request.method_mut() = http::Method::POST;
    *request.uri_mut() = "/upload?q=1".parse().unwrap();
    *request.version_mut() = http::Version::HTTP_2;
    request
        .headers_mut()
        .insert("x-test", "keep".parse().unwrap());
    request.extensions_mut().insert(42_u64);
    let request = ready(prepare_request(request, 3, BodyStorage::Text)).unwrap();
    assert_eq!(request.body(), "\u{20ac}");
    assert_eq!(request.method(), http::Method::POST);
    assert_eq!(request.uri(), "/upload?q=1");
    assert_eq!(request.version(), http::Version::HTTP_2);
    assert_eq!(request.headers()["x-test"], "keep");
    assert_eq!(request.extensions().get::<u64>(), Some(&42));
    assert!(request.extensions().get::<RequestBodyFile>().is_none());
    assert_eq!(
        ready(prepare_request(text(b""), 0, BodyStorage::Text))
            .unwrap()
            .body(),
        ""
    );
}

#[test]
fn declared_oversize_never_polls_or_opens_upload_storage() {
    let body = || {
        StreamBody::new(stream::poll_fn(
            |_| -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
                panic!("oversized declared body must not be polled")
            },
        ))
    };
    let root = tempfile::tempdir().unwrap();
    let upload = root.path().join("not-created");
    for storage in [BodyStorage::Text, BodyStorage::File(&upload)] {
        let request = Request::builder()
            .header("content-length", "5")
            .body(body())
            .unwrap();
        let error = ready(prepare_request(request, 4, storage)).unwrap_err();
        assert_eq!(error.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error.message, "request body exceeds 4 bytes");
    }
    assert!(!upload.exists());
}

#[test]
fn actual_frames_are_bounded_and_stream_failures_are_not_reinterpreted() {
    let root = tempfile::tempdir().unwrap();
    for storage in [BodyStorage::Text, BodyStorage::File(root.path())] {
        let request = frames(vec![
            Ok(Frame::data(Bytes::from_static(b"12"))),
            Ok(Frame::data(Bytes::from_static(b"345"))),
        ]);
        let error = ready(prepare_request(request, 4, storage)).unwrap_err();
        assert_eq!(error.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error.message, "request body exceeds 4 bytes");
    }
    for storage in [BodyStorage::Text, BodyStorage::File(root.path())] {
        let request = frames(vec![
            Ok(Frame::data(Bytes::from_static(b"ok"))),
            Err(io::Error::other("disconnected")),
        ]);
        let error = ready(prepare_request(request, 4, storage)).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.message, "invalid request body: disconnected");
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn binary_upload_is_owned_by_the_request_and_its_clones() {
    let error = ready(prepare_request(text(b"\0\xff"), 2, BodyStorage::Text)).unwrap_err();
    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert!(error.message.starts_with("invalid UTF-8 body:"));
    let root = tempfile::tempdir().unwrap();
    let request = ready(prepare_request(
        text(b"\0\xff"),
        2,
        BodyStorage::File(root.path()),
    ))
    .unwrap();
    assert!(request.body().is_empty());
    let path = request
        .extensions()
        .get::<RequestBodyFile>()
        .unwrap()
        .path()
        .to_owned();
    assert_eq!(std::fs::read(&path).unwrap(), b"\0\xff");
    let retained = request.extensions().clone();
    drop(request);
    assert!(Path::new(&path).exists());
    drop(retained);
    assert!(!Path::new(&path).exists());
}

#[test]
fn replacing_a_body_cannot_inherit_an_upload_lease() {
    let root = tempfile::tempdir().unwrap();
    let request = ready(prepare_request(
        text(b"old"),
        3,
        BodyStorage::File(root.path()),
    ))
    .unwrap();
    let path = request
        .extensions()
        .get::<RequestBodyFile>()
        .unwrap()
        .path()
        .to_owned();
    let request = request.map(|_| Full::new(Bytes::from_static(b"new")));
    let request = ready(prepare_request(request, 3, BodyStorage::Text)).unwrap();
    assert_eq!(request.body(), "new");
    assert!(request.extensions().get::<RequestBodyFile>().is_none());
    assert!(!Path::new(&path).exists());
}

#[test]
fn upload_configuration_errors_are_unavailable_and_do_not_consume_the_body() {
    let body = StreamBody::new(stream::pending::<Result<Frame<Bytes>, io::Error>>());
    let error = ready(prepare_request(
        Request::new(body),
        8,
        BodyStorage::File(Path::new("relative")),
    ))
    .unwrap_err();
    assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.message, "TERLAN_SERVE_UPLOAD_ROOT must be absolute");
}

#[test]
fn cancellation_while_reading_or_handling_releases_the_upload() {
    let root = tempfile::tempdir().unwrap();
    {
        let body = StreamBody::new(
            stream::iter([Ok::<_, io::Error>(Frame::data(Bytes::from_static(
                b"partial",
            )))])
            .chain(stream::pending()),
        );
        let mut ingress = pin!(prepare_request(
            Request::new(body),
            8,
            BodyStorage::File(root.path())
        ));
        assert!(ingress
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    {
        let mut handler = pin!(async {
            let request = prepare_request(text(b"complete"), 8, BodyStorage::File(root.path()))
                .await
                .unwrap();
            std::future::pending::<()>().await;
            drop(request);
        });
        assert!(handler
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn non_utf8_upload_path_is_rejected_and_the_completed_file_is_removed() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let upload = root
        .path()
        .join(std::ffi::OsString::from_vec(b"upload-\xff".to_vec()));
    let error = ready(prepare_request(
        text(b"body"),
        4,
        BodyStorage::File(&upload),
    ))
    .unwrap_err();
    assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.message, "temporary request path is not UTF-8");
    assert_eq!(std::fs::read_dir(upload).unwrap().count(), 0);
}
