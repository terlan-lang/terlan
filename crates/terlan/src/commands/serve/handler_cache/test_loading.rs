//! Test-only image admission, including remaining static-plan fixtures.
use super::*;

impl AotHandlerRuntime {
    pub(in crate::commands::serve) fn load(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
    ) -> Result<Self, String> {
        let sessions = session_service::test_session_service()?;
        Self {
            module,
            generation: Arc::new(AotHandlerGeneration::load(image, sessions)?),
            router: router.map(materialize_router).transpose()?,
            primary_request_projection: None,
            request_projections: HashMap::new(),
        }
        .admit_source_router()
    }

    pub(super) fn load_with_shard_count(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
        shard_count: usize,
    ) -> Result<Self, String> {
        let sessions = session_service::test_session_service()?;
        Self {
            module,
            generation: Arc::new(AotHandlerGeneration::load_with_shard_count(
                image,
                sessions,
                shard_count,
            )?),
            router: router.map(materialize_router).transpose()?,
            primary_request_projection: None,
            request_projections: HashMap::new(),
        }
        .admit_source_router()
    }
}
