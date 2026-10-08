use std::collections::VecDeque;
use std::fmt::Debug;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use terlan_runtime_abi::{CallbackInvocation, CallbackState, NativeValue as V};

#[derive(Clone, Copy, Debug)]
pub enum Wait {
    Text,
    Bytes,
}

pub type State = CallbackState<V, Wait>;

#[derive(Debug)]
pub struct Executor<E> {
    pub calls: Vec<(E, Option<V>, Vec<V>)>,
    pub results: VecDeque<Result<State, String>>,
    pub pending: Option<Wait>,
    pub cancellations: Vec<String>,
    pub cancel_error: bool,
    pub wait_error: bool,
    pub resume_error: bool,
}

impl<E> Default for Executor<E> {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            results: VecDeque::new(),
            pending: None,
            cancellations: Vec::new(),
            cancel_error: false,
            wait_error: false,
            resume_error: false,
        }
    }
}

impl<E> Executor<E> {
    pub fn returns(&mut self, value: V) {
        self.results.push_back(Ok(State::Complete(value)));
    }

    fn next(&mut self) -> Result<State, String> {
        let state = self
            .results
            .pop_front()
            .unwrap_or(Ok(State::Complete(V::Unit)))?;
        self.pending = match &state {
            State::Waiting(wait) => Some(*wait),
            State::Complete(_) => None,
        };
        Ok(state)
    }
}

impl<E: Copy + Debug + Send> CallbackInvocation<E> for Executor<E> {
    type Value = V;
    type Wait = Wait;
    type Wake = (Wait, String);

    fn is_waiting(&self) -> bool {
        self.pending.is_some()
    }

    fn pending_wait(&self) -> Result<Option<Wait>, String> {
        if self.wait_error {
            Err("wait failed".into())
        } else {
            Ok(self.pending)
        }
    }

    fn validate_text_wait(wait: &Wait) -> Result<(), String> {
        match wait {
            Wait::Text => Ok(()),
            Wait::Bytes => Err("cannot wake Bytes".into()),
        }
    }

    fn text_wake(wait: Wait, text: String) -> Self::Wake {
        (wait, text)
    }

    fn invoke(&mut self, event: E, callback: Option<&V>, args: Vec<V>) -> Result<State, String> {
        if self.pending.is_some() {
            return Err("busy".into());
        }
        self.calls.push((event, callback.cloned(), args));
        self.next()
    }

    async fn invoke_suspendable(
        &mut self,
        event: E,
        callback: &V,
        args: Vec<V>,
    ) -> Result<V, String> {
        match self.invoke(event, Some(callback), args)? {
            State::Complete(value) => Ok(value),
            State::Waiting(_) => Err("still waiting".into()),
        }
    }

    fn resume(&mut self, (_, text): Self::Wake) -> Result<State, String> {
        self.pending.take().ok_or("not waiting")?;
        if self.resume_error {
            return Err("resume failed".into());
        }
        Ok(State::Complete(V::String(text)))
    }

    fn cancel_pending(&mut self, reason: String) -> Result<(), String> {
        self.pending = None;
        self.cancellations.push(reason);
        if self.cancel_error {
            Err("cancel failed".into())
        } else {
            Ok(())
        }
    }
}

pub fn ready<T>(future: impl Future<Output = T>) -> T {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("scripted callback must complete in one poll"),
    }
}
