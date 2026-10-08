//! Generic VM execution adapter for package callback protocols.

use super::*;
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::native_value::from_native;
use terlan_runtime_abi::{CallbackInvocation, NativeValue};

impl<Event: Copy + Debug + Send> CallbackInvocation<Event> for AotChannelInvocation<Event> {
    type Value = ReplValue;
    type Wait = PureNativeIoWait;
    type Wake = PureNativeIoWake;

    fn is_waiting(&self) -> bool {
        self.is_waiting()
    }

    fn pending_wait(&self) -> Result<Option<Self::Wait>, String> {
        self.pending_wait()
    }

    fn validate_text_wait(wait: &Self::Wait) -> Result<(), String> {
        if wait.boundary_type() == &TvmBoundaryType::String {
            Ok(())
        } else {
            Err(format!("cannot wake {:?}", wait.boundary_type()))
        }
    }

    fn text_wake(wait: Self::Wait, text: String) -> Self::Wake {
        wait.wake(ReplValue::String(text))
    }

    fn invoke(
        &mut self,
        event: Event,
        callback: Option<&ReplValue>,
        args: Vec<NativeValue>,
    ) -> Result<AotChannelCallbackState, String> {
        self.invoke(event, callback, args.into_iter().map(from_native).collect())
    }

    async fn invoke_suspendable(
        &mut self,
        event: Event,
        callback: &ReplValue,
        args: Vec<NativeValue>,
    ) -> Result<ReplValue, String> {
        self.invoke_suspendable(event, callback, args.into_iter().map(from_native).collect())
            .await
    }

    fn resume(&mut self, wake: Self::Wake) -> Result<AotChannelCallbackState, String> {
        self.resume(wake)
    }

    fn cancel_pending(&mut self, reason: String) -> Result<(), String> {
        self.cancel_pending(reason)
    }
}
