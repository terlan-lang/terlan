use super::*;
use std::cell::RefCell;
use std::io;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Waker};

use futures_util::stream;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::Frame;

use crate::callback_test_support::{ready, Executor};
use crate::channel_plan::{WebSocketEndpointPlan, WebSocketEvent};
use crate::request_ingress::RequestBodyFile;
use crate::websocket::callbacks::WebSocketCallbacks;
use crate::websocket::session::Session;

type Socket = WebSocketCallbacks<Executor<WebSocketEvent>>;

struct Sse {
    reasons: Rc<RefCell<Vec<String>>>,
    fail: bool,
}

impl SseCancellation for Sse {
    fn cancel(&mut self, reason: String) -> Result<(), crate::ServiceError> {
        self.reasons.borrow_mut().push(reason);
        if self.fail {
            Err("cleanup error".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct App {
    file: bool,
    route_error: bool,
    suspend_result: RefCell<Option<Result<Option<Response<Bytes>>, String>>>,
    park: bool,
    immediate_error: bool,
    channel: RefCell<Option<Channel<Socket, Sse>>>,
    calls: RefCell<Vec<&'static str>>,
    body: RefCell<Option<String>>,
    upload: RefCell<Option<String>>,
}

impl Application for App {
    type WebSocket = Socket;
    type Sse = Sse;

    fn requires_file_body(&self, method: &str, path: &str) -> Result<bool, String> {
        assert_eq!(method, "POST");
        assert!(path == "/handler" || path.is_empty());
        self.calls.borrow_mut().push("route");
        if self.route_error {
            Err("route error".into())
        } else {
            Ok(self.file)
        }
    }

    async fn handle_suspendable(
        &self,
        request: &Request<String>,
    ) -> Result<Option<Response<Bytes>>, String> {
        self.calls.borrow_mut().push("suspendable");
        *self.body.borrow_mut() = Some(request.body().clone());
        if let Some(upload) = request.extensions().get::<RequestBodyFile>() {
            assert!(Path::new(upload.path()).exists());
            *self.upload.borrow_mut() = Some(upload.path().to_owned());
        }
        if self.park {
            std::future::pending::<()>().await;
        }
        self.suspend_result.borrow_mut().take().unwrap_or(Ok(None))
    }

    fn handle(
        &self,
        _: Request<String>,
        channel: &mut Option<Channel<Socket, Sse>>,
    ) -> Result<Response<Bytes>, String> {
        self.calls.borrow_mut().push("immediate");
        *channel = self.channel.borrow_mut().take();
        if self.immediate_error {
            Err("handler error".into())
        } else {
            Ok(Response::new(Bytes::from_static(b"immediate")))
        }
    }
}

fn request(body: &'static [u8]) -> Request<Full<Bytes>> {
    Request::builder()
        .method("POST")
        .uri("/handler?q=1")
        .body(Full::new(Bytes::from_static(body)))
        .unwrap()
}

fn check(response: Response<ResponseBody>, status: StatusCode, expected: &str) {
    assert_eq!(response.status(), status);
    if status != StatusCode::OK {
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
    }
    let bytes = ready(response.into_body().collect()).unwrap().to_bytes();
    assert_eq!(std::str::from_utf8(&bytes).unwrap(), expected);
}

fn socket(fail: bool) -> Socket {
    let executor = Executor {
        cancel_error: fail,
        ..Executor::default()
    };
    WebSocketCallbacks::open(
        executor,
        Session::open(WebSocketEndpointPlan::new(2, 64).unwrap()),
    )
    .unwrap()
}

#[test]
fn declared_overflow_precedes_route_resolution_and_body_polling() {
    let app = App {
        route_error: true,
        ..App::default()
    };
    let body = StreamBody::new(stream::poll_fn(
        |_| -> std::task::Poll<Option<Result<Frame<Bytes>, io::Error>>> {
            panic!("rejected request must not poll body")
        },
    ));
    let mut oversized = request(b"").map(|_| body);
    oversized
        .headers_mut()
        .insert("content-length", "9".parse().unwrap());
    check(
        ready(handle(&app, oversized, 8, None, None)),
        StatusCode::PAYLOAD_TOO_LARGE,
        "request body exceeds 8 bytes",
    );
    assert!(app.calls.borrow().is_empty());
    let mut request = request(b"12345");
    request
        .headers_mut()
        .insert("content-length", "5".parse().unwrap());
    check(
        ready(handle(&app, request, 4, None, None)),
        StatusCode::PAYLOAD_TOO_LARGE,
        "request body exceeds 4 bytes",
    );
    assert!(app.calls.borrow().is_empty());
}

#[test]
fn route_upload_configuration_and_body_errors_precede_handler_execution() {
    for (app, body, status, message) in [
        (
            App {
                route_error: true,
                ..App::default()
            },
            b"".as_slice(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "route error",
        ),
        (
            App {
                file: true,
                ..App::default()
            },
            b"",
            StatusCode::SERVICE_UNAVAILABLE,
            "TERLAN_SERVE_UPLOAD_ROOT is required for file-backed request bodies",
        ),
        (
            App::default(),
            b"12345",
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body exceeds 4 bytes",
        ),
    ] {
        check(
            ready(handle(&app, request(body), 4, None, None)),
            status,
            message,
        );
        assert_eq!(*app.calls.borrow(), ["route"]);
    }
}

#[test]
fn immediate_and_suspendable_dispatch_preserve_response_and_do_not_double_execute() {
    let app = App::default();
    check(
        ready(handle(&app, request(b"payload"), 7, None, None)),
        StatusCode::OK,
        "immediate",
    );
    assert_eq!(app.body.borrow().as_deref(), Some("payload"));
    assert_eq!(*app.calls.borrow(), ["route", "suspendable", "immediate"]);
    for result in [
        Ok(Some(Response::new(Bytes::from_static(b"suspended")))),
        Err("suspension error".into()),
    ] {
        let failed = result.is_err();
        let app = App {
            suspend_result: RefCell::new(Some(result)),
            ..App::default()
        };
        check(
            ready(handle(&app, request(b""), 0, None, None)),
            if failed {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::OK
            },
            if failed {
                "suspension error"
            } else {
                "suspended"
            },
        );
        assert_eq!(*app.calls.borrow(), ["route", "suspendable"]);
    }
    let app = App {
        immediate_error: true,
        ..App::default()
    };
    check(
        ready(handle(&app, request(b""), 0, None, None)),
        StatusCode::INTERNAL_SERVER_ERROR,
        "handler error",
    );
}

#[test]
fn upload_lifetime_ends_on_handler_completion_error_and_cancellation() {
    let root = tempfile::tempdir().unwrap();
    for immediate_error in [false, true] {
        let app = App {
            file: true,
            immediate_error,
            ..App::default()
        };
        let response = ready(handle(&app, request(b"\xff\0"), 2, Some(root.path()), None));
        assert_eq!(
            response.status(),
            if immediate_error {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::OK
            }
        );
        assert_eq!(app.body.borrow().as_deref(), Some(""));
        assert!(!Path::new(app.upload.borrow().as_ref().unwrap()).exists());
    }
    let app = App {
        file: true,
        park: true,
        ..App::default()
    };
    {
        let mut future = pin!(handle(&app, request(b"abc"), 3, Some(root.path()), None));
        assert!(future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert!(Path::new(app.upload.borrow().as_ref().unwrap()).exists());
    }
    assert!(!Path::new(app.upload.borrow().as_ref().unwrap()).exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn sse_rejection_or_handler_error_cancels_once_and_preserves_cleanup_failure() {
    for immediate_error in [false, true] {
        for fail in [false, true] {
            let reasons = Rc::new(RefCell::new(Vec::new()));
            let app = App {
                immediate_error,
                channel: RefCell::new(Some(Channel::Sse(Sse {
                    reasons: Rc::clone(&reasons),
                    fail,
                }))),
                ..App::default()
            };
            let reason = if immediate_error {
                "handler error"
            } else {
                "error[serve_http.upgrade_adapter_missing]: maintained async Hyper adapter is required for SSE"
            };
            let expected = if fail {
                format!("{reason}; cancellation failed: cleanup error")
            } else {
                reason.into()
            };
            check(
                ready(handle(&app, request(b""), 0, None, None)),
                if immediate_error {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else {
                    StatusCode::NOT_IMPLEMENTED
                },
                &expected,
            );
            assert_eq!(*reasons.borrow(), [reason]);
        }
    }
}

#[test]
fn websocket_handoff_requires_transport_and_cleans_up_handler_failure() {
    for fail in [false, true] {
        let app = App {
            channel: RefCell::new(Some(Channel::WebSocket(socket(fail)))),
            ..App::default()
        };
        let response = ready(handle(&app, request(b""), 0, None, None));
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        let message = ready(response.into_body().collect()).unwrap().to_bytes();
        assert!(
            std::str::from_utf8(&message)
                .unwrap()
                .contains("cancellation failed")
                == fail
        );
        let app = App {
            immediate_error: true,
            channel: RefCell::new(Some(Channel::WebSocket(socket(fail)))),
            ..App::default()
        };
        check(
            ready(handle(&app, request(b""), 0, None, None)),
            StatusCode::INTERNAL_SERVER_ERROR,
            if fail {
                "handler error; cancellation failed: cancel failed"
            } else {
                "handler error"
            },
        );
    }
    for uri in ["/handler?q=1", "example.com:80"] {
        let slot = UpgradeSlot::default();
        let app = App {
            channel: RefCell::new(Some(Channel::WebSocket(socket(false)))),
            ..App::default()
        };
        let mut request = request(b"");
        *request.uri_mut() = uri.parse().unwrap();
        check(
            ready(handle(&app, request, 0, None, Some(&slot))),
            StatusCode::OK,
            "immediate",
        );
        assert!(slot.take().is_some());
    }
}
