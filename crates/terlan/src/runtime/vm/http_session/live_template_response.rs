use super::{
    validate_live_template_patch_payload, ReplValue, VmHttpSession,
    VmHttpSessionLiveTemplateActorBinding, VmHttpSessionLiveTemplateSourceSpan,
    VmHttpSessionRuntime,
};
use crate::runtime::vm::http::{render_http_template_response, VmHttpTemplateResponse};
use crate::runtime::vm::http_static::{VmHttp1ResponseStream, VmHttpStreamPlan};

/// Source-aware plan for rendering one actor-bound template response.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VmHttpSessionLiveTemplateRenderPlan<'a> {
    pub(crate) template_id: &'a str,
    pub(crate) state_key: &'a str,
    pub(crate) template_name: &'a str,
    pub(crate) source_file: &'a str,
    pub(crate) source: &'a VmHttpSessionLiveTemplateSourceSpan,
    pub(crate) status: http::StatusCode,
}

/// HTTP response rendered from a versioned VM actor-state binding.
#[derive(Debug)]
pub(crate) struct VmHttpSessionLiveTemplateRenderedResponse {
    pub(crate) binding: VmHttpSessionLiveTemplateActorBinding,
    pub(crate) response: http::Response<String>,
}

/// Bounded HTTP/1 stream plan for one actor-bound typed template response.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VmHttpSessionLiveTemplateStreamPlan<'a> {
    pub(crate) response: VmHttpSessionLiveTemplateRenderPlan<'a>,
    pub(crate) chunk_size: usize,
    pub(crate) max_pending_writes: usize,
    pub(crate) close_connection: bool,
}

/// Open actor-bound template stream accepting typed rendered chunks.
#[derive(Debug)]
pub(crate) struct VmHttpSessionLiveTemplateOpenStream {
    binding: VmHttpSessionLiveTemplateActorBinding,
    stream: VmHttp1ResponseStream,
    source: VmHttpSessionLiveTemplateSourceSpan,
    template_id: String,
}

/// Finished actor-bound response ready for VM TCP scheduling.
#[derive(Debug)]
pub(crate) struct VmHttpSessionLiveTemplateRenderedStream {
    pub(crate) binding: VmHttpSessionLiveTemplateActorBinding,
    pub(crate) stream: VmHttp1ResponseStream,
}

impl VmHttpSessionLiveTemplateOpenStream {
    /// Renders and atomically admits one bounded body chunk.
    pub(crate) fn enqueue_rendered_chunk(
        &mut self,
        render: impl FnOnce(&ReplValue) -> Result<String, String>,
    ) -> Result<usize, String> {
        let state_value = self
            .binding
            .state_value
            .as_ref()
            .expect("open live-template stream retains validated actor state");
        let rendered = render(state_value).map_err(|detail| {
            live_template_actor_bind_error(
                &self.source,
                &self.template_id,
                format!("stream render failed: {detail}"),
            )
        })?;
        self.stream.enqueue(rendered.into_bytes()).map_err(|error| {
            live_template_stream_unavailable(&self.source, &self.template_id, error)
        })
    }

    /// Stops chunk admission and returns the scheduler-owned HTTP/1 stream.
    pub(crate) fn finish(mut self) -> Result<VmHttpSessionLiveTemplateRenderedStream, String> {
        self.stream.finish().map_err(|error| {
            live_template_stream_unavailable(&self.source, &self.template_id, error)
        })?;
        Ok(VmHttpSessionLiveTemplateRenderedStream {
            binding: self.binding,
            stream: self.stream,
        })
    }

    /// Cancels an open stream and discards every admitted body chunk.
    pub(crate) fn abort(mut self) -> Result<usize, String> {
        self.stream.abort().map_err(|error| {
            live_template_stream_unavailable(&self.source, &self.template_id, error)
        })
    }
}

impl VmHttpSessionRuntime {
    /// Resolves actor state and renders it through the typed VM HTTP boundary.
    pub(crate) fn render_live_template_actor_state_response(
        &mut self,
        session: &VmHttpSession,
        plan: VmHttpSessionLiveTemplateRenderPlan<'_>,
        render: impl FnOnce(&ReplValue) -> Result<String, String>,
    ) -> Result<VmHttpSessionLiveTemplateRenderedResponse, String> {
        let binding = self.resolve_live_template_actor_binding(session, plan)?;
        let state_value = binding
            .state_value
            .as_ref()
            .expect("resolved live-template binding retains actor state");
        let rendered_body = render(state_value).map_err(|detail| {
            live_template_actor_bind_error(
                plan.source,
                &binding.template_id,
                format!("render failed: {detail}"),
            )
        })?;
        let template =
            VmHttpTemplateResponse::typed(plan.template_name, plan.source_file, rendered_body)
                .map_err(|detail| {
                    live_template_actor_bind_error(plan.source, &binding.template_id, detail)
                })?;
        let response = render_http_template_response(template, plan.status).map_err(|detail| {
            live_template_actor_bind_error(plan.source, &binding.template_id, detail)
        })?;
        Ok(VmHttpSessionLiveTemplateRenderedResponse { binding, response })
    }

    /// Opens a typed actor-bound template on the VM HTTP/1 stream lane.
    pub(crate) fn open_live_template_actor_state_stream(
        &mut self,
        session: &VmHttpSession,
        plan: VmHttpSessionLiveTemplateStreamPlan<'_>,
    ) -> Result<VmHttpSessionLiveTemplateOpenStream, String> {
        let stream_plan =
            VmHttpStreamPlan::new(plan.chunk_size, plan.max_pending_writes).map_err(|error| {
                live_template_stream_unavailable(
                    plan.response.source,
                    plan.response.template_id,
                    error,
                )
            })?;
        let template = VmHttpTemplateResponse::typed(
            plan.response.template_name,
            plan.response.source_file,
            String::new(),
        )
        .map_err(|detail| {
            live_template_actor_bind_error(plan.response.source, plan.response.template_id, detail)
        })?;
        let response = render_http_template_response(template, plan.response.status)
            .map_err(|detail| {
                live_template_actor_bind_error(
                    plan.response.source,
                    plan.response.template_id,
                    detail,
                )
            })?
            .map(|_| ());
        let binding = self.resolve_live_template_actor_binding(session, plan.response)?;
        let stream = stream_plan
            .open_http1_stream(response, plan.close_connection)
            .map_err(|error| {
                live_template_stream_unavailable(
                    plan.response.source,
                    plan.response.template_id,
                    error,
                )
            })?;
        Ok(VmHttpSessionLiveTemplateOpenStream {
            binding,
            stream,
            source: plan.response.source.clone(),
            template_id: plan.response.template_id.to_string(),
        })
    }

    fn resolve_live_template_actor_binding(
        &mut self,
        session: &VmHttpSession,
        plan: VmHttpSessionLiveTemplateRenderPlan<'_>,
    ) -> Result<VmHttpSessionLiveTemplateActorBinding, String> {
        crate::runtime::vm::live_template_protocol::validate_vm_live_template_protocol_manifest(
            &self.live_template_protocol,
        )?;
        let binding = self
            .bind_live_template_to_actor_state(session, plan.template_id, plan.state_key)
            .map_err(|detail| {
                live_template_actor_bind_error(plan.source, plan.template_id, detail)
            })?;
        let state_value = binding.state_value.as_ref().ok_or_else(|| {
            live_template_actor_bind_error(
                plan.source,
                &binding.template_id,
                format!("actor state `{}` is unavailable", binding.state_key),
            )
        })?;
        validate_live_template_patch_payload(state_value, plan.source)?;
        Ok(binding)
    }
}

fn live_template_actor_bind_error(
    source: &VmHttpSessionLiveTemplateSourceSpan,
    template_id: &str,
    detail: impl std::fmt::Display,
) -> String {
    format!(
        "template_runtime_actor_bind_error: {}:{}:{}: HTTP live-template `{template_id}` {detail}",
        source.module, source.line, source.column
    )
}

fn live_template_stream_unavailable(
    source: &VmHttpSessionLiveTemplateSourceSpan,
    template_id: &str,
    detail: impl std::fmt::Debug,
) -> String {
    format!(
        "template_runtime_unavailable: {}:{}:{}: HTTP live-template `{template_id}` stream {detail:?}",
        source.module, source.line, source.column
    )
}
