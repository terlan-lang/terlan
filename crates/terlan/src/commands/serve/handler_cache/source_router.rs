//! Admit executed package descriptors; never interpret router builder syntax.
use super::AotHandlerRuntime;
use terlan_http_native::source_descriptor;
use terlan_runtime_abi::NativeAdapterError;

#[cfg(test)]
#[path = "source_router_test.rs"]
mod tests;

impl AotHandlerRuntime {
    pub(super) fn admit_source_router(mut self) -> Result<Self, String> {
        let export = format!("{}.router", self.module);
        if !self.generation.has_export(&export, 0) {
            return Ok(self);
        }
        // Evaluate once per immutable image generation. The returned closures own
        // their captures; no startup actor or mutable execution shard is retained.
        let value = self.generation.image.spawn_shard()?.call(&export, &[])?;
        let plan = source_descriptor::router(&value, |callback, arity| {
            self.validate_callable(&self.module, callback, arity)
                .map_err(|error| NativeAdapterError::new("http.callback", error.to_string(), 0))?;
            Ok(callback.clone())
        })
        .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?;
        self.router = Some(plan.into_routing_table()?);
        Ok(self)
    }
}
