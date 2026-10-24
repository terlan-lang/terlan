use super::*;
use crate::admitted_response::AdmittedResponse;
use std::collections::VecDeque;
use terlan_runtime_abi::{native_record, NativeValue as V};

pub(super) fn response(payload: &str) -> V {
    let kind = 0_i64;
    let status = 200_i64;
    let content_type = "text/plain";
    let headers = Vec::<(String, String)>::new();
    let default_headers = headers.clone();
    let chunks = Vec::<String>::new();
    let chunk_size = 16_i64;
    let max_pending_writes = 1_i64;
    native_record!(Response, {kind, payload, status, content_type, default_headers, headers, chunks, chunk_size, max_pending_writes})
}

pub(super) struct Script {
    pub(super) replies: VecDeque<(&'static str, Result<V, String>)>,
    pub(super) calls: Vec<(V, Vec<V>)>,
    pub(super) arity: Option<usize>,
    pub(super) admitted: usize,
}

impl Script {
    pub(super) fn new(
        replies: impl IntoIterator<Item = (&'static str, Result<V, String>)>,
    ) -> Self {
        Self {
            replies: replies.into_iter().collect(),
            calls: vec![],
            arity: Some(1),
            admitted: 0,
        }
    }
}

impl Host for Script {
    type Value = V;
    type Response = AdmittedResponse;

    fn invoke(&mut self, callable: &V, args: Vec<V>) -> Result<V, String> {
        self.calls.push((callable.clone(), args));
        let (expected, result) = self
            .replies
            .pop_front()
            .expect("unexpected callback execution");
        assert_eq!(callable, &V::from(expected));
        result
    }
    fn arity(&self, _: &V) -> Option<usize> {
        self.arity
    }
    fn request(&self, params: &[(String, String)]) -> V {
        params.to_vec().into()
    }
    fn scalar(value: V) -> V {
        value
    }
    fn admit(&mut self, value: V) -> Result<AdmittedResponse, String> {
        self.admitted += 1;
        AdmittedResponse::from_source(value, |_, _| panic!("no file authority granted"))
            .map_err(|error| error.message().to_owned())
    }
}

pub(super) fn router(middleware: &[&str], response: &[&str], recovery: bool) -> Router<V> {
    let router = Router::new()
        .scoped_target(
            RouteMethod::Get,
            "/item/{id:Int}",
            RouteTarget::Handler("handler".into()),
            middleware.iter().map(|name| V::from(*name)).collect(),
            response.iter().map(|name| V::from(*name)).collect(),
        )
        .unwrap();
    if recovery {
        router.error("recover".into())
    } else {
        router
    }
}
