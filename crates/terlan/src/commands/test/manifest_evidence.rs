//! Binds per-test callable observations to the compiled image and source bytes.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::runtime::native_image::debug::{
    inspect_tvm_native_debug, tvm_debug_source_sha256, TvmNativeDebugRecord,
};

/// Debug identities from the exact image used by a native test run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct NativeCallableEvidence {
    schema: &'static str,
    image_sha256: String,
    declarations: Vec<NativeDeclarationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct NativeDeclarationEvidence {
    callable_id: String,
    module: String,
    function: String,
    arity: usize,
    source_file: String,
    source_sha256: String,
    source_origin: String,
    span_start: usize,
    span_end: usize,
    continuation_ids: Vec<String>,
}

/// Failure to read or decode a compiled test image's declaration evidence.
#[derive(Debug)]
pub(super) enum NativeEvidenceError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Decode {
        path: PathBuf,
        message: String,
    },
    Identity {
        path: PathBuf,
    },
}

impl fmt::Display for NativeEvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                f,
                "error[test.evidence.read]: cannot read `{}`: {source}",
                path.display()
            ),
            Self::Decode { path, message } => write!(
                f,
                "error[test.evidence.decode]: cannot decode `{}`: {message}",
                path.display()
            ),
            Self::Identity { path } => write!(
                f,
                "error[test.evidence.identity]: `{}` does not match the executed native image",
                path.display()
            ),
        }
    }
}

impl std::error::Error for NativeEvidenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Decode { .. } | Self::Identity { .. } => None,
        }
    }
}

impl NativeCallableEvidence {
    /// Reads source identities without treating image membership as execution.
    pub(super) fn read(
        path: &Path,
        expected_digest: Option<&[u8; 32]>,
    ) -> Result<Self, NativeEvidenceError> {
        let image = std::fs::read(path).map_err(|source| NativeEvidenceError::Read {
            path: path.to_owned(),
            source,
        })?;
        let digest: [u8; 32] = Sha256::digest(&image).into();
        if expected_digest != Some(&digest) {
            return Err(NativeEvidenceError::Identity {
                path: path.to_owned(),
            });
        }
        let records =
            inspect_tvm_native_debug(&image).map_err(|message| NativeEvidenceError::Decode {
                path: path.to_owned(),
                message,
            })?;
        let mut declarations = records
            .into_iter()
            .map(NativeDeclarationEvidence::from)
            .collect::<Vec<_>>();
        declarations.sort_by(|left, right| left.callable_id.cmp(&right.callable_id));
        Ok(Self {
            schema: "terlan.native-callable-evidence/v1",
            image_sha256: tvm_debug_source_sha256(&image),
            declarations,
        })
    }
}

impl From<TvmNativeDebugRecord> for NativeDeclarationEvidence {
    fn from(record: TvmNativeDebugRecord) -> Self {
        Self {
            // String encoding preserves all 64 identity bits in JSON consumers.
            callable_id: record.callable_id.to_string(),
            module: record.module,
            function: record.function,
            arity: record.arity,
            source_file: record.source_file,
            source_sha256: record.source_sha256,
            source_origin: record.source_origin,
            span_start: record.span_start,
            span_end: record.span_end,
            continuation_ids: record.continuation_ids.iter().map(u64::to_string).collect(),
        }
    }
}
