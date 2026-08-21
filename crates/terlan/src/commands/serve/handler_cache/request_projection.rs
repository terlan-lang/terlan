//! Admission and lookup of compiler-proven opaque Request projections.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::vm::aot_metadata::{AotRouterPlan, NativeRequestProjection};
use crate::runtime::vm::http_session::VmHttpSessionService;

use super::{
    materialize_router, AdmittedRequestProjection, AotHandlerGeneration, AotHandlerRuntime,
};

impl AotHandlerRuntime {
    pub(super) fn load_with_request_projections(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
        projections: Vec<NativeRequestProjection>,
        sessions: VmHttpSessionService,
    ) -> Result<Self, String> {
        let mut primary_request_projection = None;
        let mut request_projections = HashMap::<String, HashMap<usize, _>>::new();
        for projection in projections
            .into_iter()
            .filter(|projection| projection.module == module)
        {
            let admitted = AdmittedRequestProjection {
                fields: projection.fields,
                scalar_entry: projection.scalar_entry,
                scalar_field: projection.scalar_field,
                suspending: projection.suspending,
            };
            if primary_request_projection.is_none() {
                primary_request_projection = Some(super::PrimaryRequestProjection {
                    function: projection.function,
                    arity: projection.arity,
                    projection: admitted,
                });
            } else {
                request_projections
                    .entry(projection.function)
                    .or_default()
                    .insert(projection.arity, admitted);
            }
        }
        Ok(Self {
            module,
            generation: Arc::new(AotHandlerGeneration::load(image, sessions)?),
            router: router.map(materialize_router).transpose()?,
            primary_request_projection,
            request_projections,
        })
    }

    /// Returns a narrow projection only for a direct export in this exact
    /// admitted generation. Router execution can select several callables, so
    /// it deliberately retains the complete Request until combined router
    /// proofs are available.
    pub(in crate::commands::serve) fn request_projection(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> RequestFieldProjection {
        if module != self.module || self.router.is_some() {
            return RequestFieldProjection::Complete;
        }
        self.admitted_request_projection(function, arity)
            .map(|projection| projection.fields)
            .unwrap_or(RequestFieldProjection::Complete)
    }

    /// Returns the exact handler proof when static router dispatch has already
    /// established that no middleware or recovery callback can observe the
    /// Request envelope.
    pub(in crate::commands::serve) fn direct_request_projection(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> RequestFieldProjection {
        if module != self.module {
            return RequestFieldProjection::Complete;
        }
        self.admitted_request_projection(function, arity)
            .map(|projection| projection.fields)
            .unwrap_or(RequestFieldProjection::Complete)
    }

    fn admitted_request_projection(
        &self,
        function: &str,
        arity: usize,
    ) -> Option<&AdmittedRequestProjection> {
        if let Some(primary) = &self.primary_request_projection {
            if primary.function == function && primary.arity == arity {
                return Some(&primary.projection);
            }
        }
        self.request_projections
            .get(function)
            .and_then(|arities| arities.get(&arity))
    }

    /// Returns a generated scalar ingress only when it matches this exact
    /// source export and its admitted projection proof.
    pub(super) fn scalar_request_ingress(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> Option<(&str, usize)> {
        if module != self.module || self.router.is_some() {
            return None;
        }
        let projection = self
            .primary_request_projection
            .as_ref()
            .filter(|primary| primary.function == function && primary.arity == arity)
            .map(|primary| &primary.projection)
            .or_else(|| {
                self.request_projections
                    .get(function)
                    .and_then(|arities| arities.get(&arity))
            })?;
        Some((
            projection.scalar_entry.as_deref()?,
            projection.scalar_field?,
        ))
    }

    /// Returns suspension behavior for a handler selected by a router route
    /// whose middleware contract has separately been proven empty.
    pub(in crate::commands::serve) fn direct_request_handler_may_suspend(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> bool {
        module == self.module
            && self
                .admitted_request_projection(function, arity)
                .is_some_and(|projection| projection.suspending)
    }

    /// Proves a matched router route can invoke its handler directly without
    /// skipping request middleware, response middleware, or error recovery.
    pub(in crate::commands::serve) fn direct_router_handler_is_safe(
        &self,
        method: &str,
        path: &str,
        module: &str,
        function: &str,
        arity: usize,
    ) -> bool {
        let Some(router) = &self.router else {
            return true;
        };
        if router.error_handler().is_some() {
            return false;
        }
        let Some(method) = crate::runtime::vm::http_router::VmHttpRouteMethod::from_name(method)
        else {
            return false;
        };
        let Ok(crate::runtime::vm::http_router::VmHttpRouterOutcome::Matched(dispatch)) =
            router.dispatch(method, path)
        else {
            return false;
        };
        if !dispatch.middleware.is_empty() || !dispatch.response_middleware.is_empty() {
            return false;
        }
        let crate::runtime::vm::http_router::VmHttpRouteTarget::Handler(callable) =
            &dispatch.target
        else {
            return false;
        };
        crate::runtime::vm::http_router::VmHttpCompiledCallableRef::from_value(callable)
            .is_some_and(|callable| {
                callable.module == module
                    && callable.function == function
                    && callable.arity == arity
            })
    }
}
