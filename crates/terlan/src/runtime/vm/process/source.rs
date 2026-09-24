//! Source identities and locations retained by VM process inspection.

/// Source identity for runtime inspection and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessSource {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) source_path: Option<String>,
}

/// Current VM execution location retained for inspection and debugging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessLocation {
    pub(crate) source: VmProcessSource,
    pub(crate) instruction_offset: usize,
}

impl VmProcessSource {
    /// Creates source identity metadata for a process.
    pub(crate) fn new(
        module: impl Into<String>,
        function: impl Into<String>,
        arity: usize,
    ) -> Self {
        Self {
            module: module.into(),
            function: function.into(),
            arity,
            source_path: None,
        }
    }

    /// Attaches an explicit source path to runtime-owned source metadata.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn with_source_path(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }
}

impl VmProcessLocation {
    /// Renders one stable source-facing VM stack frame.
    #[cfg(test)]
    pub(crate) fn render(&self) -> String {
        let identity = format!(
            "{}.{}/{}",
            self.source.module, self.source.function, self.source.arity
        );
        match &self.source.source_path {
            Some(path) => format!(
                "{identity} [{}] @vm:{}",
                escape_source_path(path),
                self.instruction_offset
            ),
            None => format!("{identity} @vm:{}", self.instruction_offset),
        }
    }
}

#[cfg(test)]
fn escape_source_path(path: &str) -> String {
    path.chars().flat_map(char::escape_debug).collect()
}
