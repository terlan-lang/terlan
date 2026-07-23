//! Admission and lookup of compiler-proven opaque Request projections.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::compiler::native_ir::NativeRequestProjection;
use crate::compiler::router::AotRouterPlan;
use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};

use super::{materialize_router, AotHandlerGeneration, AotHandlerRuntime};

impl AotHandlerRuntime {
    pub(super) fn load_with_request_projections(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
        projections: Vec<NativeRequestProjection>,
    ) -> Result<Self, String> {
        let sessions = VmHttpSessionService::new(VmHttpSessionRuntime::new("terlc-serve", 86_400)?);
        let mut primary_request_projection = None;
        let mut request_projections = HashMap::<String, HashMap<usize, _>>::new();
        for projection in projections
            .into_iter()
            .filter(|projection| projection.module == module)
        {
            if primary_request_projection.is_none() {
                primary_request_projection = Some(super::PrimaryRequestProjection {
                    function: projection.function,
                    arity: projection.arity,
                    fields: projection.fields,
                });
            } else {
                request_projections
                    .entry(projection.function)
                    .or_default()
                    .insert(projection.arity, projection.fields);
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
                return primary.fields;
            }
        }
        self.request_projections
            .get(function)
            .and_then(|arities| arities.get(&arity))
            .copied()
            .unwrap_or(RequestFieldProjection::Complete)
    }
}
