//! Admission and lookup of compiler-proven opaque Request projections.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::compiler::native_ir::NativeRequestProjection;
use crate::compiler::router::AotRouterPlan;
use crate::runtime::native::http::RequestFieldProjection;
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
        if let Some(primary) = &self.primary_request_projection {
            if primary.function == function && primary.arity == arity {
                return primary.projection.fields;
            }
        }
        self.request_projections
            .get(function)
            .and_then(|arities| arities.get(&arity))
            .map(|projection| projection.fields)
            .unwrap_or(RequestFieldProjection::Complete)
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

    /// Returns compiler-proven suspension behavior for this exact generation.
    pub(in crate::commands::serve) fn request_handler_may_suspend(
        &self,
        module: &str,
        function: &str,
        arity: usize,
    ) -> bool {
        if module != self.module || self.router.is_some() {
            return false;
        }
        self.primary_request_projection
            .as_ref()
            .filter(|primary| primary.function == function && primary.arity == arity)
            .map(|primary| primary.projection.suspending)
            .or_else(|| {
                self.request_projections
                    .get(function)
                    .and_then(|arities| arities.get(&arity))
                    .map(|projection| projection.suspending)
            })
            .unwrap_or(false)
    }
}
