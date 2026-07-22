#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
#[path = "io_diagnostics_test.rs"]
mod io_diagnostics_test;

static NEXT_DIAGNOSTIC_LOG_ID: AtomicU64 = AtomicU64::new(1);

/// Stable code for an I/O resource removed before interest was deselected.
pub(crate) const VM_IO_RESOURCE_REMOVED_WITHOUT_DESELECTING: &str =
    "vm.io.resource_removed_without_deselecting";

/// VM-owned source-map identity for I/O diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmIoDiagnosticSourceMap {
    pub(crate) source_map_id: String,
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) source_file: String,
    pub(crate) start_line: u32,
    pub(crate) start_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_column: u32,
}

impl VmIoDiagnosticSourceMap {
    /// Creates source-map identity metadata for one I/O diagnostic.
    pub(crate) fn new(
        source_map_id: impl Into<String>,
        module: impl Into<String>,
        function: impl Into<String>,
        source_file: impl Into<String>,
        span: VmIoDiagnosticSpan,
    ) -> Result<Self, String> {
        let source_map_id = source_map_id.into();
        let module = module.into();
        let function = function.into();
        let source_file = source_file.into();
        if source_map_id.trim().is_empty() {
            return Err("VM I/O diagnostic source_map_id cannot be empty".to_string());
        }
        if module.trim().is_empty() {
            return Err("VM I/O diagnostic module cannot be empty".to_string());
        }
        if function.trim().is_empty() {
            return Err("VM I/O diagnostic function cannot be empty".to_string());
        }
        if source_file.trim().is_empty() {
            return Err("VM I/O diagnostic source_file cannot be empty".to_string());
        }
        span.validate()?;
        Ok(Self {
            source_map_id,
            module,
            function,
            source_file,
            start_line: span.start_line,
            start_column: span.start_column,
            end_line: span.end_line,
            end_column: span.end_column,
        })
    }

    /// Renders a stable source-map location for CLI/debugger output.
    pub(crate) fn render_source_map_location(&self) -> String {
        format!(
            "{}:{}:{}-{}:{} [{}::{} @ {}]",
            self.source_file,
            self.start_line,
            self.start_column,
            self.end_line,
            self.end_column,
            self.module,
            self.function,
            self.source_map_id
        )
    }
}

/// One-based source span used by VM I/O diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmIoDiagnosticSpan {
    pub(crate) start_line: u32,
    pub(crate) start_column: u32,
    pub(crate) end_line: u32,
    pub(crate) end_column: u32,
}

impl VmIoDiagnosticSpan {
    /// Creates one-based source span metadata.
    pub(crate) fn new(start_line: u32, start_column: u32, end_line: u32, end_column: u32) -> Self {
        Self {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    fn validate(self) -> Result<(), String> {
        if self.start_line == 0
            || self.start_column == 0
            || self.end_line == 0
            || self.end_column == 0
        {
            return Err("VM I/O diagnostic source span must be one-based".to_string());
        }
        if (self.end_line, self.end_column) < (self.start_line, self.start_column) {
            return Err("VM I/O diagnostic source span cannot move backwards".to_string());
        }
        Ok(())
    }
}

/// Stable I/O resource identity for diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmIoDiagnosticResource {
    pub(crate) kind: VmIoDiagnosticResourceKind,
    pub(crate) handle: String,
}

impl VmIoDiagnosticResource {
    /// Creates diagnostic resource metadata.
    pub(crate) fn new(
        kind: VmIoDiagnosticResourceKind,
        handle: impl Into<String>,
    ) -> Result<Self, String> {
        let handle = handle.into();
        if handle.trim().is_empty() {
            return Err("VM I/O diagnostic resource handle cannot be empty".to_string());
        }
        Ok(Self { kind, handle })
    }
}

/// Resource categories that can emit source-map-aware I/O diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmIoDiagnosticResourceKind {
    TcpStream,
    UdpSocket,
    PackageDownload,
    Timer,
    HttpHandler,
    WebSocket,
    TlsConnection,
}

/// Runtime-visible I/O diagnostic severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmIoDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

/// One source-map-aware VM I/O diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmIoDiagnostic {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) severity: VmIoDiagnosticSeverity,
    pub(crate) operation: String,
    pub(crate) resource: VmIoDiagnosticResource,
    pub(crate) source_map: VmIoDiagnosticSourceMap,
}

impl VmIoDiagnostic {
    /// Creates one VM I/O diagnostic with source-map metadata.
    pub(crate) fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        severity: VmIoDiagnosticSeverity,
        operation: impl Into<String>,
        resource: VmIoDiagnosticResource,
        source_map: VmIoDiagnosticSourceMap,
    ) -> Result<Self, String> {
        let code = code.into();
        let message = message.into();
        let operation = operation.into();
        if code.trim().is_empty() {
            return Err("VM I/O diagnostic code cannot be empty".to_string());
        }
        if message.trim().is_empty() {
            return Err("VM I/O diagnostic message cannot be empty".to_string());
        }
        if operation.trim().is_empty() {
            return Err("VM I/O diagnostic operation cannot be empty".to_string());
        }
        Ok(Self {
            code,
            message,
            severity,
            operation,
            resource,
            source_map,
        })
    }

    /// Renders one stable source-facing diagnostic line.
    pub(crate) fn render_text(&self) -> String {
        format!(
            "{:?} {} during {} on {:?} {}: {} at {}",
            self.severity,
            self.code,
            self.operation,
            self.resource.kind,
            self.resource.handle,
            self.message,
            self.source_map.render_source_map_location()
        )
    }
}

/// VM-owned I/O diagnostic collection.
#[derive(Debug)]
pub(crate) struct VmIoDiagnosticLog {
    id: u64,
    diagnostics: Vec<VmIoDiagnostic>,
}

impl Default for VmIoDiagnosticLog {
    fn default() -> Self {
        Self {
            id: NEXT_DIAGNOSTIC_LOG_ID.fetch_add(1, Ordering::Relaxed),
            diagnostics: Vec::new(),
        }
    }
}

/// Installed typed diagnostic query over one exact log generation.
#[derive(Debug)]
pub(crate) struct VmIoDiagnosticProbe {
    log_id: u64,
    start_index: usize,
    code: String,
    closed: bool,
}

impl VmIoDiagnosticLog {
    /// Creates an empty diagnostic log.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records one source-map-aware I/O diagnostic.
    pub(crate) fn record_diagnostic(&mut self, diagnostic: VmIoDiagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Installs a probe that observes only subsequently recorded diagnostics.
    pub(crate) fn install_probe(
        &self,
        code: impl Into<String>,
    ) -> Result<VmIoDiagnosticProbe, String> {
        let code = code.into();
        if code.trim().is_empty() {
            return Err("VM I/O diagnostic probe code cannot be empty".to_string());
        }
        Ok(VmIoDiagnosticProbe {
            log_id: self.id,
            start_index: self.diagnostics.len(),
            code,
            closed: false,
        })
    }

    /// Returns diagnostics tied to one source-map id.
    pub(crate) fn diagnostics_for_source_map(&self, source_map_id: &str) -> Vec<VmIoDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.source_map.source_map_id == source_map_id)
            .cloned()
            .collect()
    }
}

impl VmIoDiagnosticProbe {
    /// Returns whether the installed code has appeared since installation.
    pub(crate) fn matched(&self, log: &VmIoDiagnosticLog) -> Result<bool, String> {
        self.validate_log(log)?;
        if self.closed {
            return Err("VM I/O diagnostic probe is closed".to_string());
        }
        Ok(log.diagnostics[self.start_index..]
            .iter()
            .any(|diagnostic| diagnostic.code == self.code))
    }

    /// Closes this probe after validating its owning diagnostic log.
    pub(crate) fn close(&mut self, log: &VmIoDiagnosticLog) -> Result<(), String> {
        self.validate_log(log)?;
        if self.closed {
            return Err("VM I/O diagnostic probe is already closed".to_string());
        }
        self.closed = true;
        Ok(())
    }

    fn validate_log(&self, log: &VmIoDiagnosticLog) -> Result<(), String> {
        if self.log_id != log.id {
            return Err("VM I/O diagnostic probe belongs to a different log".to_string());
        }
        Ok(())
    }
}
