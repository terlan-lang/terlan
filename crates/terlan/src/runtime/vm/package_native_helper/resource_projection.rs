//! Source handles are local registry identities, never unchecked worker tokens.

use super::{direct_std, result_ok, ReplValue, VmRuntimeResult};
use crate::runtime::vm::pure_native::{
    managed_capability_term, repl_value_to_boundary_term, PureNativeCapabilityRequest,
};
use crate::terlan_native_boundary::term::{
    NativeBoundaryReplyTerm as Reply, NativeBoundaryTerm as Term,
};
use terlan_runtime_abi::{
    NativeAdapterError, NativeResourceHandle as Handle, NativeResourceOperation, ResourceRegistry,
};

struct Remote {
    handle: Handle,
    type_name: &'static str,
}

#[derive(Default)]
pub(super) struct Adapter {
    resources: ResourceRegistry<Remote>,
}

#[derive(Debug)]
pub(super) struct Projection {
    contract: &'static NativeResourceOperation,
    receiver: Option<Handle>,
}

impl Adapter {
    pub(super) fn prepare(
        &self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<(Vec<Term>, Projection)> {
        let contract = crate::std_native_packages::resource_operation(&request.operation)
            .ok_or("error[native_package.operation]: unregistered resource operation")?;
        if request.capability != "package-native" {
            return Err(
                "error[native_package.capability]: resource operation requires package-native"
                    .into(),
            );
        }
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[native_package.arguments]: resource call has no source arguments")?;
        if args.len() != contract.arity {
            return Err("error[native_package.arity]: resource call arity mismatch".into());
        }
        let mut receiver = None;
        let mut terms = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            if let ReplValue::Record { fields, .. } = arg {
                if fields.iter().any(|(name, _)| name.starts_with("$native_")) {
                    let (local, type_name, claimed_owner) = direct_std::native_handle(fields)
                        .ok_or("error[native_package.handle]: malformed source handle")??;
                    if fields.len() != 4
                        || claimed_owner != owner.to_string()
                        || type_name != contract.resource_type
                    {
                        return Err(
                            "error[native_package.handle]: resource owner or type mismatch".into(),
                        );
                    }
                    let remote = self
                        .resources
                        .get_for_owner(local, owner)
                        .map_err(NativeAdapterError::from)
                        .map_err(adapter_error)?;
                    if remote.type_name != type_name {
                        return Err(
                            "error[native_package.handle]: registered resource type mismatch"
                                .into(),
                        );
                    }
                    if index == 0 && contract.mutates_receiver {
                        receiver = Some(local);
                    }
                    terms.push(Term::Handle {
                        id: remote.handle.id,
                        generation: remote.handle.generation,
                    });
                    continue;
                }
            }
            reject_nested_resources(arg)?;
            terms.push(repl_value_to_boundary_term(arg.clone())?);
        }
        Ok((terms, Projection { contract, receiver }))
    }

    pub(super) fn complete(
        &mut self,
        owner: u64,
        projection: Projection,
        reply: Reply,
    ) -> VmRuntimeResult<Reply> {
        let contract = projection.contract;
        let value = match reply {
            Reply::Error {
                code,
                message,
                offset,
            } => {
                if code == "resource_worker.lost" {
                    self.close_owner(owner);
                    return Ok(Reply::Error {
                        code,
                        message,
                        offset,
                    });
                }
                if code.starts_with("capability")
                    || code.starts_with("dispatch.")
                    || code.starts_with("native_boundary.")
                    || code.starts_with("resource.")
                {
                    return Ok(Reply::Error {
                        code,
                        message,
                        offset,
                    });
                }
                let Some(error_name) = contract.result_error else {
                    return Ok(Reply::Error {
                        code,
                        message,
                        offset,
                    });
                };
                let error = direct_std::typed_result_error(
                    NativeAdapterError::new(code, message, offset),
                    error_name,
                );
                return repl_value_to_boundary_term(error).map(Reply::Ok);
            }
            Reply::Ok(Term::Handle { id, generation }) if contract.returns_resource => {
                if id == 0 || generation == 0 {
                    return Err("error[native_package.reply]: zero resource identity".into());
                }
                let remote = Handle { id, generation };
                let local = match projection.receiver {
                    Some(local) => {
                        let existing = self
                            .resources
                            .get_for_owner(local, owner)
                            .map_err(NativeAdapterError::from)
                            .map_err(adapter_error)?;
                        if existing.handle != remote {
                            return Err(
                                "error[native_package.reply]: mutation replaced receiver identity"
                                    .into(),
                            );
                        }
                        local
                    }
                    None => self
                        .resources
                        .insert_for_owner(
                            owner,
                            Remote {
                                handle: remote,
                                type_name: contract.resource_type,
                            },
                        )
                        .map_err(NativeAdapterError::from)
                        .map_err(adapter_error)?,
                };
                direct_std::native_handle_value(owner, local, contract.resource_type)?
            }
            Reply::Ok(term) if !contract.returns_resource => managed_capability_term(term)?,
            Reply::Ok(_) => {
                return Err("error[native_package.reply]: package result kind mismatch".into())
            }
        };
        repl_value_to_boundary_term(if contract.result_error.is_some() {
            result_ok(value)
        } else {
            value
        })
        .map(Reply::Ok)
    }

    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
    }
}

fn adapter_error(error: NativeAdapterError) -> super::VmRuntimeError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}

fn reject_nested_resources(value: &ReplValue) -> VmRuntimeResult<()> {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            ReplValue::Record { fields, .. } => {
                if fields.iter().any(|(name, _)| name.starts_with("$native_")) {
                    return Err("error[native_package.handle]: nested resource handles are not value arguments".into());
                }
                pending.extend(fields.iter().map(|(_, value)| value));
            }
            ReplValue::List(values) | ReplValue::Tuple(values) => pending.extend(values),
            ReplValue::Map(entries) => {
                pending.extend(entries.iter().flat_map(|(key, value)| [key, value]))
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "resource_projection_test.rs"]
mod tests;
