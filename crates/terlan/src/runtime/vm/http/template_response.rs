use std::io::Write;

use super::write_http1_response;
use crate::runtime::vm::{
    memory::{
        VmMemoryAccountant, VmMemoryPressureOutcome, VmSharedAllocationId, VmSharedAllocationKind,
    },
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
};
use crate::terlan_html::{artifact_template_target_from_path, ArtifactTemplateTarget};

/// Typed rendered template response produced by a VM HTTP handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpTemplateResponse {
    pub(crate) template_name: String,
    pub(crate) source_file: String,
    pub(crate) rendered_body: String,
    target: ArtifactTemplateTarget,
}

#[cfg(test)]
impl VmHttpTemplateResponse {
    pub(crate) fn typed(
        template_name: impl Into<String>,
        source_file: impl Into<String>,
        rendered_body: impl Into<String>,
    ) -> Result<Self, String> {
        let template_name = template_name.into();
        if template_name.trim().is_empty() {
            return Err("VM HTTP template response name cannot be empty".to_string());
        }
        let source_file = source_file.into();
        if source_file.trim().is_empty() {
            return Err("VM HTTP template response source file cannot be empty".to_string());
        }
        let target = artifact_template_target_from_path(&source_file).ok_or_else(|| {
            format!(
                "template_runtime_unsupported_target: VM HTTP template source `{source_file}` has no supported .terl.* target suffix"
            )
        })?;
        Ok(Self {
            template_name,
            source_file,
            rendered_body: rendered_body.into(),
            target,
        })
    }

    pub(crate) fn html(
        template_name: impl Into<String>,
        source_file: impl Into<String>,
        rendered_body: impl Into<String>,
    ) -> Result<Self, String> {
        let template = Self::typed(template_name, source_file, rendered_body)?;
        if !template.target.parses_to_html_tree() {
            return Err(format!(
                "template_runtime_unsupported_target: VM HTTP HTML response cannot render {} template `{}`",
                template.target.name(),
                template.source_file
            ));
        }
        Ok(template)
    }
}

pub(crate) fn render_http_template_response(
    template: VmHttpTemplateResponse,
    status: ::http::StatusCode,
) -> Result<::http::Response<String>, String> {
    build_template_response(template, status)
}

/// Typed failure from VM-owned template and response-buffer accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmAccountedHttpOutputError {
    Template(String),
    Memory(String),
    MemoryPressureRejected,
    Response(String),
    Write(String),
}

/// Rendered template whose body remains owned by a VM process.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VmAccountedHttpTemplateResponse {
    template: VmHttpTemplateResponse,
    owner: VmProcessId,
    allocation: VmSharedAllocationId,
}

#[cfg(test)]
impl VmAccountedHttpTemplateResponse {
    pub(crate) fn typed(
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        template_name: impl Into<String>,
        source_file: impl Into<String>,
        rendered_body: impl Into<String>,
    ) -> Result<Self, VmAccountedHttpOutputError> {
        let template = VmHttpTemplateResponse::typed(template_name, source_file, rendered_body)
            .map_err(VmAccountedHttpOutputError::Template)?;
        Self::account(memory, scheduler, processes, owner, template)
    }
    pub(crate) fn html(
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        template_name: impl Into<String>,
        source_file: impl Into<String>,
        rendered_body: impl Into<String>,
    ) -> Result<Self, VmAccountedHttpOutputError> {
        let template = VmHttpTemplateResponse::html(template_name, source_file, rendered_body)
            .map_err(VmAccountedHttpOutputError::Template)?;
        Self::account(memory, scheduler, processes, owner, template)
    }

    fn account(
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        template: VmHttpTemplateResponse,
    ) -> Result<Self, VmAccountedHttpOutputError> {
        let body_bytes = template.rendered_body.len();
        let decision = memory
            .register_shared_allocation(
                processes,
                owner,
                VmSharedAllocationKind::TemplateOutput,
                body_bytes,
            )
            .map_err(VmAccountedHttpOutputError::Memory)?;
        scheduler
            .charge_memory_reductions(processes, owner, body_bytes)
            .map_err(VmAccountedHttpOutputError::Memory)?;
        if decision.pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Err(VmAccountedHttpOutputError::MemoryPressureRejected);
        }
        let allocation = decision.allocation_id.ok_or_else(|| {
            VmAccountedHttpOutputError::Memory(
                "accounted template output did not produce an allocation id".to_string(),
            )
        })?;
        Ok(Self {
            template,
            owner,
            allocation,
        })
    }

    pub(crate) fn render(
        self,
        memory: &mut VmMemoryAccountant,
        status: ::http::StatusCode,
    ) -> Result<VmAccountedHttpResponse, VmAccountedHttpOutputError> {
        memory
            .reclassify_shared_allocation(
                self.allocation,
                self.owner,
                VmSharedAllocationKind::TemplateOutput,
                VmSharedAllocationKind::ResponseBuffer,
            )
            .map_err(VmAccountedHttpOutputError::Memory)?;
        let response = build_template_response(self.template, status)
            .map_err(VmAccountedHttpOutputError::Response)?;
        Ok(VmAccountedHttpResponse {
            response,
            owner: self.owner,
            allocation: self.allocation,
        })
    }

    pub(crate) fn cancel(
        self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
    ) -> Result<(), VmAccountedHttpOutputError> {
        release_output(
            memory,
            scheduler,
            processes,
            self.owner,
            self.allocation,
            self.template.rendered_body.len(),
        )
    }
}

/// HTTP response retaining its VM-owned response-buffer allocation until write.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VmAccountedHttpResponse {
    response: ::http::Response<String>,
    owner: VmProcessId,
    allocation: VmSharedAllocationId,
}

#[cfg(test)]
impl VmAccountedHttpResponse {
    pub(crate) fn allocation(&self) -> VmSharedAllocationId {
        self.allocation
    }

    pub(crate) fn write(
        self,
        memory: &mut VmMemoryAccountant,
        scheduler: &mut VmScheduler,
        processes: &mut VmProcessTable,
        writer: &mut dyn Write,
        close_connection: bool,
    ) -> Result<(), VmAccountedHttpOutputError> {
        let body_bytes = self.response.body().len();
        let write_result = write_http1_response(writer, &self.response, close_connection);
        release_output(
            memory,
            scheduler,
            processes,
            self.owner,
            self.allocation,
            body_bytes,
        )?;
        write_result.map_err(VmAccountedHttpOutputError::Write)
    }
}

fn build_template_response(
    template: VmHttpTemplateResponse,
    status: ::http::StatusCode,
) -> Result<::http::Response<String>, String> {
    ::http::Response::builder()
        .status(status)
        .header(::http::header::CONTENT_TYPE, template.target.content_type())
        .header("x-terlan-template", template.template_name)
        .body(template.rendered_body)
        .map_err(|error| format!("failed to build VM HTTP template response: {error}"))
}

#[cfg(test)]
fn release_output(
    memory: &mut VmMemoryAccountant,
    scheduler: &mut VmScheduler,
    processes: &mut VmProcessTable,
    owner: VmProcessId,
    allocation: VmSharedAllocationId,
    logical_bytes: usize,
) -> Result<(), VmAccountedHttpOutputError> {
    memory
        .release_shared_allocation(processes, allocation, owner)
        .map_err(VmAccountedHttpOutputError::Memory)?;
    scheduler
        .charge_memory_reductions(processes, owner, logical_bytes)
        .map_err(VmAccountedHttpOutputError::Memory)?;
    Ok(())
}
