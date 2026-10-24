//! Schema projection from the same source descriptors admitted by serving.

use super::{ApiContract, ApiRoute};
use crate::service_error::ServiceError;
use crate::source_descriptor::{RouteTarget, Router};
use terlan_runtime_abi::DescriptorValue;

impl ApiContract {
    /// Projects evaluated source routes, validating the serving contract first.
    ///
    /// The host supplies only callable identities, never route-building rules.
    /// Channel schemas remain unsupported and fail instead of disappearing.
    pub fn from_source_router<C: DescriptorValue + Clone>(
        service_name: impl Into<String>,
        version: impl Into<String>,
        router: Router<C>,
        mut label: impl FnMut(&C) -> Result<String, String>,
    ) -> Result<Self, ServiceError> {
        router
            .clone()
            .into_routing_table()
            .map_err(|error| ServiceError::from(error.to_string()))?;
        let mut routes = Vec::with_capacity(router.routes.len());
        for route in router.routes {
            let RouteTarget::Handler(handler) = route.target else {
                return Err(format!(
                    "error[api_emit.channel_schema]: channel schema is not supported for {} {}",
                    route.method, route.path
                )
                .into());
            };
            routes.push(ApiRoute {
                method: route.method,
                path: route.path,
                handler: label(&handler)?,
            });
        }
        if let Some(fallback) = router.fallback {
            let handler = label(&fallback.handler)?;
            for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
                // A concrete wildcard route takes precedence over the fallback.
                if !routes
                    .iter()
                    .any(|route| route.method == method && route.path == "/*")
                {
                    routes.push(ApiRoute {
                        method: method.into(),
                        path: "/*".into(),
                        handler: handler.clone(),
                    });
                }
            }
        }
        Ok(Self::from_routes(service_name, version, routes))
    }
}

#[cfg(test)]
#[path = "source_test.rs"]
mod tests;
