//! Package-owned callback ordering; invocation and wakeup stay with the host.

use super::*;
use std::future::Future;

#[cfg(test)]
#[path = "execution_test.rs"]
mod tests;

/// An admitted route's callback chain, independent of transport and VM types.
pub struct HandlerPipeline<'a, C> {
    pub handler: &'a C,
    pub middleware: &'a [C],
    pub response_middleware: &'a [C],
    pub recovery: Option<&'a C>,
}

impl<C: Clone> HandlerPipeline<'_, C> {
    /// Runs middleware in declaration order and unwinds response middleware.
    /// Dropping this future drops the host's pending invocation as well.
    pub async fn execute<V, F>(
        &self,
        request: V,
        handler_args: Vec<V>,
        mut invoke: impl FnMut(C, Vec<V>) -> F,
        error_value: impl Fn(&NativeAdapterError) -> V,
    ) -> Result<V>
    where
        V: DescriptorValue + Clone,
        F: Future<Output = Result<V>>,
    {
        let mut response = None;
        for middleware in self.middleware {
            let result = invoke(middleware.clone(), vec![request.clone()]).await;
            match result.and_then(middleware_result) {
                Ok(None) => {}
                Ok(Some(value)) => {
                    response = Some(value);
                    break;
                }
                Err(error) => return self.recover(error, &mut invoke, &error_value).await,
            }
        }
        let mut response = match response {
            Some(value) => value,
            None => match invoke(self.handler.clone(), handler_args)
                .await
                .and_then(response_value)
            {
                Ok(value) => value,
                Err(error) => self.recover(error, &mut invoke, &error_value).await?,
            },
        };
        for middleware in self.response_middleware.iter().rev() {
            let result = invoke(middleware.clone(), vec![request.clone(), response]).await;
            response = match result.and_then(response_value) {
                Ok(value) => value,
                Err(error) => return self.recover(error, &mut invoke, &error_value).await,
            };
        }
        Ok(response)
    }

    /// Recovers a host response-conversion error through the same callback policy.
    pub async fn recover<V, F>(
        &self,
        cause: NativeAdapterError,
        invoke: &mut impl FnMut(C, Vec<V>) -> F,
        error_value: &impl Fn(&NativeAdapterError) -> V,
    ) -> Result<V>
    where
        F: Future<Output = Result<V>>,
    {
        let Some(handler) = self.recovery else {
            return Err(cause);
        };
        invoke(handler.clone(), vec![error_value(&cause)])
            .await
            .map_err(|recovery| {
                error(format!(
                    "router failed with `{}`; error handler failed with `{}`",
                    cause.message(),
                    recovery.message()
                ))
            })
    }
}

pub(crate) fn middleware_result<V: DescriptorValue + Clone>(value: V) -> Result<Option<V>> {
    match value.descriptor_view() {
        DescriptorView::Atom("continue") => Ok(None),
        DescriptorView::Record("Continue", []) => Ok(None),
        DescriptorView::Record("Respond", _) => {
            let [response] = record(&value, "Respond", ["response"])?;
            response_value(response.clone()).map(Some)
        }
        _ => Err(error("expected Continue or Respond(Response)")),
    }
}

fn response_value<V: DescriptorValue>(value: V) -> Result<V> {
    if matches!(
        value.descriptor_view(),
        DescriptorView::Record("Response", _)
    ) {
        Ok(value)
    } else {
        Err(error("expected Response"))
    }
}
