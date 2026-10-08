//! Host execution for package-owned callback protocols.

use std::fmt::Debug;
use std::future::Future;

use crate::{DescriptorValue, NativeValue};

/// A callback either completed or retained one typed host wait.
#[derive(Debug)]
pub enum CallbackState<V, W> {
    Complete(V),
    Waiting(W),
}

/// Linear callback execution supplied by an embedding runtime. Packages own
/// event selection, argument construction, result contracts, and lifecycle;
/// hosts retain opaque callable values and exact continuation authority.
pub trait CallbackInvocation<Event>: Debug {
    type Value: DescriptorValue + Debug;
    type Wait: Debug;
    type Wake;

    fn is_waiting(&self) -> bool;
    fn pending_wait(&self) -> Result<Option<Self::Wait>, String>;
    fn validate_text_wait(wait: &Self::Wait) -> Result<(), String>;
    fn text_wake(wait: Self::Wait, text: String) -> Self::Wake;

    fn invoke(
        &mut self,
        event: Event,
        callback: Option<&Self::Value>,
        args: Vec<NativeValue>,
    ) -> Result<CallbackState<Self::Value, Self::Wait>, String>;

    fn invoke_suspendable(
        &mut self,
        event: Event,
        callback: &Self::Value,
        args: Vec<NativeValue>,
    ) -> impl Future<Output = Result<Self::Value, String>> + Send;

    fn resume(
        &mut self,
        wake: Self::Wake,
    ) -> Result<CallbackState<Self::Value, Self::Wait>, String>;

    fn cancel_pending(&mut self, reason: String) -> Result<(), String>;
}
