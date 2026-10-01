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
        Poll::Pending => panic!("in-memory body unexpectedly suspended"),
    }
}

fn frames(
    items: Vec<Result<Frame<Bytes>, io::Error>>,
) -> impl Body<Data = Bytes, Error = io::Error> + Unpin {
    StreamBody::new(stream::iter(items))
}

fn data(bytes: &'static [u8]) -> Result<Frame<Bytes>, io::Error> {
    Ok(Frame::data(Bytes::from_static(bytes)))
}

#[test]
fn declared_lengths_are_an_early_check_not_a_replacement_for_frame_limits() {
    let mut headers = http::HeaderMap::new();
    assert!(!declared_body_exceeds_limit(&headers, 0));
    for value in ["bad", "18446744073709551616", " 5 ", "0"] {
        headers.append(http::header::CONTENT_LENGTH, value.parse().unwrap());
    }
    headers.append(
        http::header::CONTENT_LENGTH,
        http::HeaderValue::from_bytes(b"\xff").unwrap(),
    );
    assert!(declared_body_exceeds_limit(&headers, 4));
    assert!(!declared_body_exceeds_limit(&headers, 5));
    assert_eq!(
        ready(collect_bounded_body(Full::new(Bytes::new()), 0)),
        Ok(vec![])
    );
    assert_eq!(
        ready(collect_bounded_body(Full::new(Bytes::from_static(b"x")), 0)),
        Err(BodyReadError::TooLarge)
    );
}

#[test]
fn multiple_frames_and_trailers_preserve_binary_data_and_exact_limit() {
    let body = || {
        frames(vec![
            data(b"\0\xff"),
            data(b""),
            data(b"abc"),
            Ok(Frame::trailers(http::HeaderMap::new())),
        ])
    };
    assert_eq!(
        ready(collect_bounded_body(body(), 5)),
        Ok(b"\0\xffabc".to_vec())
    );
    assert_eq!(
        ready(collect_bounded_body(body(), 4)),
        Err(BodyReadError::TooLarge)
    );
    let root = tempfile::tempdir().unwrap();
    let file = ready(spool_bounded_body_to_root(body(), 5, root.path())).unwrap();
    assert_eq!(std::fs::read(file.path()).unwrap(), b"\0\xffabc");
    let path = file.path().to_path_buf();
    drop(file);
    assert!(!path.exists());
}

#[test]
fn oversized_frame_is_not_partially_written_or_followed_by_more_reads() {
    let body = frames(vec![
        data(b"12"),
        data(b"345"),
        Err(io::Error::other("must not be read")),
    ]);
    let mut written = Vec::new();
    assert_eq!(
        ready(copy_bounded_body(body, 4, &mut written)),
        Err(BodyReadError::TooLarge)
    );
    assert_eq!(written, b"12");
}

#[test]
fn spooled_files_are_unique_and_do_not_overwrite_existing_contents() {
    let root = tempfile::tempdir().unwrap();
    let existing = root.path().join("terlan-request-body-existing.upload");
    std::fs::write(&existing, "keep").unwrap();
    let first = ready(spool_bounded_body_to_root(
        Full::new(Bytes::from_static(b"first")),
        5,
        root.path(),
    ))
    .unwrap();
    let second = ready(spool_bounded_body_to_root(
        Full::new(Bytes::new()),
        0,
        root.path(),
    ))
    .unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(std::fs::read(first.path()).unwrap(), b"first");
    assert!(std::fs::read(second.path()).unwrap().is_empty());
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    drop((first, second));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn stream_errors_and_limit_rejections_remove_incomplete_uploads() {
    let root = tempfile::tempdir().unwrap();
    let failed = || frames(vec![data(b"ok"), Err(io::Error::other("stream failed"))]);
    assert_eq!(
        ready(collect_bounded_body(failed(), 8)),
        Err(BodyReadError::Invalid("stream failed".into()))
    );
    assert!(
        matches!(ready(spool_bounded_body_to_root(failed(), 8, root.path())), Err(BodyReadError::Invalid(message)) if message == "stream failed")
    );
    assert!(matches!(
        ready(spool_bounded_body_to_root(
            frames(vec![data(b"ok"), data(b"too much")]),
            3,
            root.path()
        )),
        Err(BodyReadError::TooLarge)
    ));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn dropping_a_suspended_upload_removes_the_file_and_does_not_spin() {
    let root = tempfile::tempdir().unwrap();
    let body = StreamBody::new(stream::iter([data(b"partial")]).chain(stream::pending()));
    {
        let mut future = pin!(spool_bounded_body_to_root(body, 8, root.path()));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        let entries = std::fs::read_dir(root.path())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(std::fs::read(entries[0].path()).unwrap(), b"partial");
        assert!(future.as_mut().poll(&mut context).is_pending());
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn invalid_roots_fail_before_polling_the_body() {
    let body = || StreamBody::new(stream::pending::<Result<Frame<Bytes>, io::Error>>());
    assert!(
        matches!(ready(spool_bounded_body_to_root(body(), 8, Path::new("relative"))), Err(BodyReadError::Unavailable(message)) if message == "TERLAN_SERVE_UPLOAD_ROOT must be absolute")
    );
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, "keep").unwrap();
    assert!(
        matches!(ready(spool_bounded_body_to_root(body(), 8, &file)), Err(BodyReadError::Unavailable(message)) if message.starts_with("cannot create upload root:"))
    );
    assert_eq!(std::fs::read(file).unwrap(), b"keep");
}

#[test]
fn suspended_bodies_resume_without_resetting_accounting_or_duplicating_frames() {
    let body = || {
        let mut next = 0;
        StreamBody::new(stream::poll_fn(move |context| {
            next += 1;
            match next {
                1 => Poll::Ready(Some(data(b"123"))),
                2 => {
                    context.waker().wake_by_ref();
                    Poll::Pending
                }
                3 => Poll::Ready(Some(data(b"45"))),
                _ => Poll::Ready(None),
            }
        }))
    };
    for limit in [4, 5] {
        let mut future = pin!(collect_bounded_body(body(), limit));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(
            future.as_mut().poll(&mut context),
            Poll::Ready(if limit == 5 {
                Ok(b"12345".to_vec())
            } else {
                Err(BodyReadError::TooLarge)
            }),
        );
        let root = tempfile::tempdir().unwrap();
        let mut future = pin!(spool_bounded_body_to_root(body(), limit, root.path()));
        assert!(future.as_mut().poll(&mut context).is_pending());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(Ok(file)) if limit == 5 => {
                assert_eq!(std::fs::read(file.path()).unwrap(), b"12345");
            }
            Poll::Ready(Err(BodyReadError::TooLarge)) if limit == 4 => {}
            other => panic!("unexpected upload outcome: {other:?}"),
        }
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[derive(Default)]
struct ScriptedWriter {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
    fail_write: bool,
    fail_flush: bool,
}

impl Write for ScriptedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        if self.fail_write {
            return Err(io::Error::other("write failed"));
        }
        if self.writes == 1 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let count = bytes.len().min(2);
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.fail_flush {
            Err(io::Error::other("flush failed"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn writer_partial_progress_interruptions_and_failures_are_not_hidden() {
    let body = || Full::new(Bytes::from_static(b"abcde"));
    let mut writer = ScriptedWriter::default();
    assert_eq!(ready(copy_bounded_body(body(), 5, &mut writer)), Ok(()));
    assert_eq!(writer.bytes, b"abcde");
    assert_eq!((writer.writes, writer.flushes), (4, 1));
    let mut writer = ScriptedWriter {
        fail_write: true,
        ..Default::default()
    };
    assert_eq!(
        ready(copy_bounded_body(body(), 5, &mut writer)),
        Err(BodyReadError::Unavailable(
            "cannot spool body: write failed".into()
        ))
    );
    assert_eq!((writer.writes, writer.flushes), (1, 0));
    let mut writer = ScriptedWriter {
        fail_flush: true,
        ..Default::default()
    };
    assert_eq!(
        ready(copy_bounded_body(body(), 5, &mut writer)),
        Err(BodyReadError::Unavailable(
            "cannot flush body: flush failed".into()
        ))
    );
    assert_eq!(writer.bytes, b"abcde");
    assert_eq!(writer.flushes, 1);
}
