//! HTTP package admission of library-independent aggregate observations.

#[cfg(test)]
#[path = "source_request_suspension_test.rs"]
mod source_request_suspension_test;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::runtime::native_image::aggregate_projection::{
    AggregateFieldProjection, NativeAggregateProjection,
};
use crate::runtime::native_image::managed::SemanticTypeId;
use terlan_http_native::session_service::{SessionHost, SessionService};
use terlan_http_native::RequestFieldProjection;

use super::{AdmittedRequestProjection, AotHandlerGeneration, AotHandlerRuntime};

impl AotHandlerRuntime {
    /// Request ingress follows the admitted source parameter identity, not the
    /// number of route captures or whether execution can suspend.
    pub(in crate::commands::serve) fn uses_source_request(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> bool {
        self.generation
            .image
            .export_parameters(module, function, arity)
            .and_then(|parameters| parameters.first())
            == Some(&terlan_runtime_abi::TvmBoundaryType::Managed(
                request_semantic_id().bytes(),
            ))
    }

    pub(super) fn load_with_aggregate_projections(
        module: String,
        image: &Path,
        projections: Vec<NativeAggregateProjection>,
        sessions: SessionService<impl SessionHost + 'static>,
    ) -> Result<Self, String> {
        let mut primary_request_projection = None;
        let mut request_projections = HashMap::<String, HashMap<usize, _>>::new();
        for projection in projections
            .into_iter()
            .filter(|projection| projection.module == module)
        {
            let Some(admitted) = admit_projection(&projection) else {
                continue;
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
        Self {
            module,
            generation: Arc::new(AotHandlerGeneration::load(image, sessions)?),
            router: None,
            primary_request_projection,
            request_projections,
        }
        .admit_source_router()
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
        module != self.module
            || self
                .admitted_request_projection(function, arity)
                .is_none_or(|projection| projection.suspending)
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
        let Some(method) = terlan_http_native::routing::RouteMethod::from_name(method) else {
            return false;
        };
        let Ok(terlan_http_native::routing::RouterOutcome::Matched(dispatch)) =
            router.dispatch(method, path)
        else {
            return false;
        };
        if !dispatch.middleware.is_empty() || !dispatch.response_middleware.is_empty() {
            return false;
        }
        let terlan_http_native::routing::RouteTarget::Handler(callable) = &dispatch.target else {
            return false;
        };
        crate::runtime::vm::native_callable::VmNativeCallableRef::from_value(callable).is_some_and(
            |callable| {
                callable.module == module
                    && callable.function == function
                    && callable.arity == arity
            },
        )
    }
}

fn admit_projection(projection: &NativeAggregateProjection) -> Option<AdmittedRequestProjection> {
    if projection.semantic != request_semantic_id() {
        return None;
    }
    let fields = match &projection.fields {
        AggregateFieldProjection::Complete => RequestFieldProjection::Complete,
        AggregateFieldProjection::Fields(fields) => {
            RequestFieldProjection::from_observed_fields(fields.iter().copied())
        }
    };
    let scalar_field = projection
        .scalar_field
        .and_then(|field| field.checked_add(1));
    let scalar = scalar_field.is_some_and(|field| fields.admits_scalar_string(field))
        && projection.scalar_entry.is_some();
    Some(AdmittedRequestProjection {
        fields,
        scalar_entry: scalar.then(|| projection.scalar_entry.clone()).flatten(),
        scalar_field: scalar.then_some(scalar_field).flatten(),
        suspending: projection.suspending,
    })
}

fn request_semantic_id() -> SemanticTypeId {
    static IDENTITY: std::sync::OnceLock<SemanticTypeId> = std::sync::OnceLock::new();
    *IDENTITY.get_or_init(|| {
        SemanticTypeId::from_canonical(RequestFieldProjection::SEMANTIC_TYPE)
            .expect("HTTP request has a nonempty canonical identity")
    })
}

#[cfg(test)]
#[path = "request_projection_test.rs"]
mod tests;
