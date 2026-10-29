//! Package-worker process lifecycle and bounded, correlated request transport.

use std::ffi::OsStr;
use std::io::BufReader;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::{
    encode_argument, legacy_reply, package_helper_environment, transport,
    PureNativeCapabilityRequest, ReplValue, VmRuntimeResult,
};

use crate::runtime::native_boundary::adapter_abi::{
    PUBLIC_ADAPTER_MAX_FRAME_BYTES, PUBLIC_ADAPTER_MAX_TRANSFER_BYTES,
};

const LIMITS: transport::Limits = transport::Limits {
    frame: PUBLIC_ADAPTER_MAX_FRAME_BYTES,
    transfer: PUBLIC_ADAPTER_MAX_TRANSFER_BYTES,
};

/// One live package helper with monotonic request correlation.
pub(super) struct VmPackageNativeHelper {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_request_id: u64,
    closed: bool,
}

impl VmPackageNativeHelper {
    /// Starts the helper selected by the package runtime environment.
    pub(super) fn from_environment(namespace: &str) -> VmRuntimeResult<Self> {
        let env_name = package_helper_environment(namespace)?;
        let path = std::env::var_os(&env_name).ok_or_else(|| {
            format!(
                "error[native_helper_unavailable]: {env_name} is not set for native package namespace `{namespace}`"
            )
        })?;
        Self::spawn(path)
    }

    /// Opens the helper's request and reply pipes for this VM-owned client.
    pub(super) fn spawn(path: impl AsRef<OsStr>) -> VmRuntimeResult<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("error[native_helper_unavailable]: {error}"))?;
        let input = child.stdin.take().ok_or_else(|| {
            "error[native_helper_unavailable]: helper stdin is closed".to_string()
        })?;
        let output = child.stdout.take().ok_or_else(|| {
            "error[native_helper_unavailable]: helper stdout is closed".to_string()
        })?;
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            next_request_id: 0,
            closed: false,
        })
    }

    /// Executes one compiler-native package request.
    pub(super) fn call(
        &mut self,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        if self.closed {
            return Err("error[native_helper_protocol]: helper transport is closed".into());
        }
        let arguments = request.package_arguments.as_ref().ok_or_else(|| {
            "error[native_helper_protocol]: built-in capability was sent to a package helper"
                .to_string()
        })?;
        let byte_count = arguments.iter().try_fold(0_usize, |total, argument| {
            let length = match argument {
                ReplValue::Bytes(bytes) => bytes.len(),
                _ => 0,
            };
            total.checked_add(length)
        });
        if byte_count.is_none_or(|count| count > LIMITS.transfer) {
            return Err(
                "error[native_helper_protocol]: binary arguments exceed transfer bound".into(),
            );
        }
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| "error[native_helper_protocol]: request id overflow".to_string())?;
        let mut fields = vec![
            "call".to_string(),
            self.next_request_id.to_string(),
            STANDARD.encode(request.operation.as_bytes()),
        ];
        fields.extend(
            arguments
                .iter()
                .map(encode_argument)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let line = fields.join(" ");
        if line.len() > LIMITS.message() {
            return Err(
                "error[native_helper_protocol]: package request exceeds transfer bound".into(),
            );
        }
        let result = transport::write_message(
            &mut self.input,
            &line,
            self.next_request_id,
            transport::Direction::Call,
            LIMITS,
        )
        .map_err(|error| format!("error[native_helper_io]: {error}").into())
        .and_then(|()| legacy_reply::read_reply(&mut self.output, self.next_request_id, LIMITS));
        if result.as_ref().is_err_and(|error| {
            let message = error.to_string();
            message.contains("error[native_helper_protocol]")
                || message.contains("error[native_helper_io]")
        }) {
            self.closed = true;
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        result
    }
}

impl Drop for VmPackageNativeHelper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
