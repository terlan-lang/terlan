//! Package-worker process lifecycle and bounded, correlated request transport.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::{
    decode_reply, encode_argument, package_helper_environment, PureNativeCapabilityRequest,
    ReplValue, VmRuntimeResult,
};

const MAX_FRAME_BYTES: usize = 1_048_576;

/// One live package helper with monotonic request correlation.
pub(super) struct VmPackageNativeHelper {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_request_id: u64,
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
        })
    }

    /// Executes one compiler-native package request.
    pub(super) fn call(
        &mut self,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let arguments = request.package_arguments.as_ref().ok_or_else(|| {
            "error[native_helper_protocol]: built-in capability was sent to a package helper"
                .to_string()
        })?;
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
        if line.len().saturating_add(1) > MAX_FRAME_BYTES {
            return Err(
                "error[native_helper_protocol]: package request exceeds one helper frame".into(),
            );
        }
        writeln!(self.input, "{line}")
            .and_then(|()| self.input.flush())
            .map_err(|error| format!("error[native_helper_io]: {error}"))?;
        let mut reply = String::new();
        let read = self
            .output
            .by_ref()
            .take((MAX_FRAME_BYTES + 1) as u64)
            .read_line(&mut reply)
            .map_err(|error| format!("error[native_helper_io]: {error}"))?;
        if read == 0 {
            return Err("error[native_helper_io]: helper exited without replying".into());
        }
        if reply.len() > MAX_FRAME_BYTES {
            return Err("error[native_helper_protocol]: helper reply is oversized".into());
        }
        decode_reply(reply.trim_end_matches(['\r', '\n']), self.next_request_id)
    }
}

impl Drop for VmPackageNativeHelper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
