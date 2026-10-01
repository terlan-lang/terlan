use super::*;
use futures_util::FutureExt;
use std::cell::RefCell;
use std::future::{ready, Future};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use terlan_runtime_abi::NativeValue as V;

fn failure_message(cause: &NativeAdapterError) -> V {
    cause.message().into()
}

fn response() -> V {
    V::Record {
        name: "Response".into(),
        fields: vec![],
    }
}

fn pipeline(recovery: bool) -> HandlerPipeline<'static, usize> {
    HandlerPipeline {
        handler: &0,
        middleware: &[1, 2],
        response_middleware: &[3, 4],
        recovery: recovery.then_some(&5),
    }
}

#[test]
fn callbacks_preserve_order_short_circuit_and_reverse_unwind() {
    for short in [false, true] {
        let calls = RefCell::new(Vec::new());
        let value = pipeline(false)
            .execute(
                V::Int(42),
                vec![V::Int(42)],
                |id, args| {
                    calls.borrow_mut().push(id);
                    assert_eq!(args[0], V::Int(42));
                    if id >= 3 {
                        assert_eq!(args[1], response());
                    }
                    ready(Ok(match id {
                        1 if short => V::Record {
                            name: "Respond".into(),
                            fields: vec![("response".into(), response())],
                        },
                        1 | 2 => V::Atom("continue".into()),
                        _ => response(),
                    }))
                },
                failure_message,
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(value, response());
        assert_eq!(
            *calls.borrow(),
            if short {
                vec![1, 4, 3]
            } else {
                vec![1, 2, 0, 4, 3]
            }
        );
    }
}

#[test]
fn failure_at_each_stage_recovers_once_without_replaying_completed_callbacks() {
    for failed in 0..5 {
        for recovery_fails in [false, true] {
            let calls = RefCell::new(Vec::new());
            let value = pipeline(true)
                .execute(
                    V::Unit,
                    vec![V::Unit],
                    |id, args| {
                        calls.borrow_mut().push(id);
                        if id == 5 {
                            assert_eq!(args, vec![V::String("failure".into())]);
                        }
                        ready(if id == failed || (id == 5 && recovery_fails) {
                            Err(error("failure"))
                        } else if id == 1 || id == 2 {
                            Ok(V::Atom("continue".into()))
                        } else {
                            Ok(response())
                        })
                    },
                    failure_message,
                )
                .now_or_never()
                .unwrap();
            assert_eq!(value.is_err(), recovery_fails);
            let mut expected = match failed {
                0 => vec![1, 2, 0, 5],
                1 => vec![1, 5],
                2 => vec![1, 2, 5],
                3 => vec![1, 2, 0, 4, 3, 5],
                _ => vec![1, 2, 0, 4, 5],
            };
            if failed == 0 && !recovery_fails {
                expected.extend([4, 3]);
            }
            assert_eq!(*calls.borrow(), expected);
        }
    }
}

#[test]
fn malformed_middleware_and_response_values_fail_closed() {
    for bad in [
        V::Unit,
        V::Atom("respond".into()),
        V::Record {
            name: "Respond".into(),
            fields: vec![("wrong".into(), response())],
        },
    ] {
        assert!(pipeline(false)
            .execute(
                V::Unit,
                vec![V::Unit],
                |_, _| ready(Ok(bad.clone())),
                failure_message
            )
            .now_or_never()
            .unwrap()
            .is_err());
    }
    for failed in [0, 3, 4] {
        assert!(pipeline(false)
            .execute(
                V::Unit,
                vec![V::Unit],
                |id, _| ready(Ok(if id == failed {
                    V::Unit
                } else if id == 1 || id == 2 {
                    V::Atom("continue".into())
                } else {
                    response()
                })),
                failure_message
            )
            .now_or_never()
            .unwrap()
            .is_err());
    }
}

struct PendingCall(Rc<RefCell<Vec<&'static str>>>);
impl Future for PendingCall {
    type Output = Result<V>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.borrow_mut().push("pending");
        Poll::Pending
    }
}
impl Drop for PendingCall {
    fn drop(&mut self) {
        self.0.borrow_mut().push("cancelled");
    }
}

#[test]
fn dropping_pipeline_drops_pending_host_call_without_starting_next_callback() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let value = pipeline(false)
        .execute(
            V::Unit,
            vec![V::Unit],
            |id, _| {
                assert_eq!(id, 1);
                events.borrow_mut().push("started");
                PendingCall(Rc::clone(&events))
            },
            failure_message,
        )
        .now_or_never();
    assert!(value.is_none());
    assert_eq!(*events.borrow(), ["started", "pending", "cancelled"]);
}
