//! Native-only dynamic HTTP handler image cache.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use crate::commands::artifacts::fingerprint;
use crate::compiler::router::{AotRouterCallable, AotRouterPlan, AotRouterRouteTarget};
use crate::runtime::vm::http_router::{
    VmHttpCompiledCallableRef, VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouter,
};
use crate::runtime::vm::http_session::{VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::ReplValue;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

use super::handler::WebPackageHandler;
use super::source_path_from_manifest;

pub(super) mod invocation;

const CACHE_ERROR: &str = "error[serve.aot.cache]: native handler cache lock poisoned";

static HANDLER_CACHE: OnceLock<Mutex<HashMap<PathBuf, HandlerCacheEntry>>> = OnceLock::new();

#[cfg(test)]
#[path = "handler_cache_generation_test.rs"]
mod handler_cache_generation_test;
#[path = "handler_cache_test_support.rs"]
pub(super) mod handler_cache_test_support;

/// One admitted native handler image. It never owns or retains compiler IR.
#[derive(Debug)]
pub(super) struct AotHandlerRuntime {
    module: String,
    image: PureNativeExecutionImage,
    router: Option<VmHttpRouter>,
}

impl AotHandlerRuntime {
    pub(in crate::commands::serve) fn load(
        module: String,
        image: &Path,
        router: Option<AotRouterPlan>,
    ) -> Result<Self, String> {
        let sessions = VmHttpSessionService::new(VmHttpSessionRuntime::new("terlc-serve", 86_400)?);
        Ok(Self {
            module,
            image: PureNativeExecutionImage::load_with_http_sessions(image, sessions)?,
            router: router.map(materialize_router).transpose()?,
        })
    }

    pub(super) fn has_function(&self, module: &str, function: &str, arity: usize) -> bool {
        module == self.module
            && ((function == "router" && arity == 0 && self.router.is_some())
                || self
                    .image
                    .has_export(&format!("{module}.{function}"), arity))
    }

    /// Executes one native callback that is required to complete immediately.
    ///
    /// A suspended callback is cancelled under its request-owned shard and
    /// rejected. External wake values may enter only through the typed
    /// invocation APIs used by request and channel event pumps.
    pub(super) fn execute_immediate_native(
        &self,
        module: &str,
        function: &str,
        args: Vec<ReplValue>,
        _output: &mut dyn FnMut(&str),
    ) -> Result<ReplValue, String> {
        match self.begin_request_invocation(module, function, args)? {
            invocation::AotHandlerInvocationStep::Complete(value) => Ok(value),
            invocation::AotHandlerInvocationStep::Waiting(invocation) => {
                let boundary = invocation.wait()?.boundary_type().clone();
                let reason = format!(
                    "error[serve.aot.async_io_unavailable]: immediate native callback suspended on {boundary:?}; use VM-owned asynchronous request orchestration"
                );
                match invocation.cancel(reason.clone()) {
                    Ok(()) => Err(reason),
                    Err(cleanup) => Err(format!(
                        "{reason}; error[serve.aot.invocation_shutdown]: {cleanup}"
                    )),
                }
            }
        }
    }

    pub(super) fn execute_callable(
        &self,
        module: &str,
        callable: &ReplValue,
        args: Vec<ReplValue>,
        output: &mut dyn FnMut(&str),
    ) -> Result<ReplValue, String> {
        let callable = VmHttpCompiledCallableRef::from_value(callable).ok_or_else(|| {
            "error[serve.aot.callable]: router value is not a static native callback".to_string()
        })?;
        if callable.module != module || args.len() != callable.arity {
            return Err(format!(
                "error[serve.aot.callable]: callback `{}.{}/{}` cannot be invoked as `{module}` with {} arguments",
                callable.module,
                callable.function,
                callable.arity,
                args.len()
            ));
        }
        self.execute_immediate_native(module, &callable.function, args, output)
            .map_err(|error| {
                format!(
                    "error[serve.aot.callable]: callback `{}.{}/{}` failed: {error}",
                    callable.module, callable.function, callable.arity
                )
            })
    }

    pub(super) fn callable_arity(&self, callable: &ReplValue) -> Option<usize> {
        VmHttpCompiledCallableRef::from_value(callable).map(|callable| callable.arity)
    }

    pub(super) fn execute_http_router(
        &self,
        module: &str,
        function: &str,
        _output: &mut dyn FnMut(&str),
    ) -> Result<VmHttpRouter, String> {
        if module != self.module || function != "router" {
            return Err(format!(
                "error[serve.aot.router]: native router `{module}.{function}/0` is not loaded"
            ));
        }
        self.router.clone().ok_or_else(|| {
            format!("error[serve.aot.router]: module `{module}` has no static router plan")
        })
    }
}

#[derive(Clone)]
struct HandlerCacheEntry {
    len: u64,
    modified: Option<SystemTime>,
    checksum: String,
    runtime: Arc<AotHandlerRuntime>,
}

/// Request lease over one immutable native handler generation.
pub(super) struct VmHandlerRuntimeLease(Arc<AotHandlerRuntime>);

impl VmHandlerRuntimeLease {
    pub(super) fn vm(&self) -> &AotHandlerRuntime {
        &self.0
    }
}

pub(super) fn cached_vm_handler_for_manifest(
    web_root: &Path,
    project_root: &Path,
    handler: &WebPackageHandler,
) -> Result<Arc<AotHandlerRuntime>, String> {
    Ok(cached_entry_for_manifest(web_root, project_root, handler)?.runtime)
}

pub(super) fn cached_vm_handler_runtime_for_manifest(
    web_root: &Path,
    project_root: &Path,
    handler: &WebPackageHandler,
) -> Result<VmHandlerRuntimeLease, String> {
    Ok(VmHandlerRuntimeLease(
        cached_entry_for_manifest(web_root, project_root, handler)?.runtime,
    ))
}

fn cached_entry_for_manifest(
    web_root: &Path,
    project_root: &Path,
    handler: &WebPackageHandler,
) -> Result<HandlerCacheEntry, String> {
    let source = handler.source.as_ref().ok_or_else(|| {
        format!(
            "error[serve.aot.source_missing]: dynamic handler `{}.{}/{}` has no source metadata",
            handler.module, handler.function, handler.arity
        )
    })?;
    let source_path = source_path_from_manifest(project_root, &source.path).ok_or_else(|| {
        format!(
            "error[serve.aot.source_path]: dynamic handler source path `{}` is unsafe",
            source.path
        )
    })?;
    cached_source_entry(web_root, &source_path, &handler.module)
}

fn cached_source_entry(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
) -> Result<HandlerCacheEntry, String> {
    let metadata = fs::metadata(source_path).map_err(|error| {
        format!(
            "error[serve.aot.source]: failed to inspect `{}`: {error}",
            source_path.display()
        )
    })?;
    let len = metadata.len();
    let modified = metadata.modified().ok();
    if let Some(entry) = cache()?
        .get(source_path)
        .filter(|entry| entry.len == len && entry.modified == modified)
        .cloned()
    {
        return Ok(entry);
    }

    let source = fs::read_to_string(source_path).map_err(|error| {
        format!(
            "error[serve.aot.source]: failed to read `{}`: {error}",
            source_path.display()
        )
    })?;
    let checksum = format!("source-fnv1a64:{:016x}", fingerprint(source.as_bytes()));
    if let Some(entry) = cache()?
        .get(source_path)
        .filter(|entry| entry.checksum == checksum)
        .cloned()
    {
        return Ok(entry);
    }

    let source_name = source_path.to_string_lossy();
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_name,
        &source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .map_err(|code| {
        format!(
            "error[serve.aot.compile]: failed to compile `{source_name}` with exit code {code:?}"
        )
    })?;
    if artifacts.core.module != expected_module {
        return Err(format!(
            "error[serve.aot.module]: source declared `{}` but manifest expected `{expected_module}`",
            artifacts.core.module
        ));
    }
    let (core, router) = crate::compiler::router::prepare_aot_router_module(&artifacts.core)?;
    let module_stem = expected_module.replace('.', "_");
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        web_root,
        &module_stem,
        &core,
    )?
    .ok_or_else(|| {
        format!(
            "error[serve.aot.image_required]: `{expected_module}` did not produce a native image; runtime CoreIR interpretation has been removed"
        )
    })?;
    let entry = HandlerCacheEntry {
        len,
        modified,
        checksum,
        runtime: Arc::new(AotHandlerRuntime::load(
            expected_module.to_string(),
            &image,
            router,
        )?),
    };
    cache()?.insert(source_path.to_path_buf(), entry.clone());
    Ok(entry)
}

/// Converts compiler-owned router metadata into the VM dispatch model.
fn materialize_router(plan: AotRouterPlan) -> Result<VmHttpRouter, String> {
    let mut router = VmHttpRouter::new();
    for middleware in plan.middleware {
        router = router.use_middleware(callable_value(middleware));
    }
    for middleware in plan.response_middleware {
        router = router.map_response(callable_value(middleware));
    }
    for route in plan.routes {
        let method = VmHttpRouteMethod::from_name(&route.method).ok_or_else(|| {
            format!(
                "error[serve.aot.router]: unsupported route method `{}`",
                route.method
            )
        })?;
        let target = match route.target {
            AotRouterRouteTarget::Handler(handler) => {
                VmHttpRouteTarget::Handler(callable_value(handler))
            }
            AotRouterRouteTarget::Sse(plan) => VmHttpRouteTarget::SseEndpoint(plan),
            AotRouterRouteTarget::WebSocket(plan) => VmHttpRouteTarget::WebSocketEndpoint(plan),
        };
        router = router.scoped_target(
            method,
            route.path,
            target,
            route.middleware.into_iter().map(callable_value).collect(),
            route
                .response_middleware
                .into_iter()
                .map(callable_value)
                .collect(),
        )?;
    }
    if let Some(fallback) = plan.fallback {
        router = router.fallback(callable_value(fallback));
    }
    if let Some(error) = plan.error {
        router = router.error(callable_value(error));
    }
    Ok(router)
}

/// Encodes one compiler callback in the closed VM router protocol.
fn callable_value(callable: AotRouterCallable) -> ReplValue {
    VmHttpCompiledCallableRef {
        module: callable.module,
        function: callable.function,
        arity: callable.arity,
    }
    .into_value()
}

fn cache() -> Result<std::sync::MutexGuard<'static, HashMap<PathBuf, HandlerCacheEntry>>, String> {
    HANDLER_CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| CACHE_ERROR.to_string())
}
