//! Source-linking failures retain their phase and underlying cause.

use std::{fmt, io, path::PathBuf};

use crate::compiler::native_ir::NativeIrError;

#[derive(Debug)]
pub(in crate::commands::emit_js) enum SourceLinkError {
    Read { path: PathBuf, source: io::Error },
    Compile { module: String },
    ModuleMismatch { expected: String, actual: String },
    DuplicateSymbol { function: String, arity: usize },
    Analysis(NativeIrError),
}

impl fmt::Display for SourceLinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "cannot read source library `{}`: {source}", path.display()),
            Self::Compile { module } => write!(formatter, "cannot compile source library `{module}` for JavaScript"),
            Self::ModuleMismatch { expected, actual } => write!(formatter, "source library `{expected}` declares `{actual}`"),
            Self::DuplicateSymbol { function, arity } => write!(formatter, "JavaScript source linking requires a unique function identity for `{function}/{arity}`"),
            Self::Analysis(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SourceLinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Analysis(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NativeIrError> for SourceLinkError {
    fn from(error: NativeIrError) -> Self {
        Self::Analysis(error)
    }
}
