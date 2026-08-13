//! Deterministic second-backend fixture for accelerator contract verification.

use std::fs;

use super::{
    artifact_kernels, sha256_bytes, AcceleratorAotArtifact, AcceleratorAotBackend,
    AcceleratorAotError, AcceleratorAotRequest, AcceleratorArtifactDescriptor,
    AcceleratorArtifactSource, ACCELERATOR_ARTIFACT_SCHEMA,
};

/// Backend-neutral synthetic vector artifact emitter used by ecosystem gates.
#[derive(Clone, Copy, Debug, Default)]
pub struct SyntheticVectorBackend;

impl AcceleratorAotBackend for SyntheticVectorBackend {
    fn identity(&self) -> &'static str {
        "synthetic-vector"
    }

    fn compile(
        &self,
        request: &AcceleratorAotRequest<'_>,
    ) -> Result<AcceleratorAotArtifact, AcceleratorAotError> {
        request
            .ir
            .verify()
            .map_err(|error| AcceleratorAotError::InvalidIr(error.to_string()))?;
        if request.architecture != "vector-v1" || request.toolchain.name != "synthetic-vector-aot" {
            return Err(AcceleratorAotError::Toolchain(
                "synthetic vector backend requires vector-v1 and synthetic-vector-aot".to_string(),
            ));
        }
        let bytes = serde_json::to_vec(request.ir)
            .map_err(|error| AcceleratorAotError::InvalidIr(error.to_string()))?;
        let ir_sha256 = sha256_bytes(&bytes);
        let artifact_sha256 = sha256_bytes(&bytes);
        let artifact = format!(
            "{}-{artifact_sha256}.vector.json",
            safe_name(&request.ir.module)
        );
        let descriptor = AcceleratorArtifactDescriptor {
            schema: ACCELERATOR_ARTIFACT_SCHEMA.to_string(),
            backend: self.identity().to_string(),
            artifact_format: "vector-object".to_string(),
            architecture: request.architecture.to_string(),
            ir_sha256,
            toolchain: request.toolchain.clone(),
            kernels: artifact_kernels(&request.ir.kernels),
            sources: request
                .ir
                .kernels
                .iter()
                .map(|kernel| AcceleratorArtifactSource {
                    entrypoint: kernel.name.clone(),
                    source: kernel.source.clone(),
                })
                .collect(),
            artifact: artifact.clone(),
            artifact_sha256,
            build_options: request.build_options.clone(),
        };
        fs::create_dir_all(request.output_directory).map_err(io_error)?;
        let artifact_path = request.output_directory.join(artifact);
        let descriptor_path = request.output_directory.join("artifact.json");
        let descriptor_bytes = serde_json::to_vec_pretty(&descriptor)
            .map_err(|error| AcceleratorAotError::InvalidArtifact(error.to_string()))?;
        let cache_hit = artifact_path.is_file()
            && descriptor_path.is_file()
            && fs::read(&artifact_path).map_err(io_error)? == bytes
            && fs::read(&descriptor_path).map_err(io_error)? == descriptor_bytes;
        if !cache_hit {
            fs::write(&artifact_path, &bytes).map_err(io_error)?;
            fs::write(&descriptor_path, &descriptor_bytes).map_err(io_error)?;
        }
        Ok(AcceleratorAotArtifact {
            descriptor,
            bytes,
            descriptor_path,
            artifact_path,
            cache_hit,
        })
    }
}

fn safe_name(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect()
}

fn io_error(error: std::io::Error) -> AcceleratorAotError {
    AcceleratorAotError::Io(error.to_string())
}
