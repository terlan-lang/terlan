use super::*;

pub(super) fn render_native_boundary_metadata(
    manifest: &CAbiBindingManifest,
) -> Result<String, String> {
    let target = crate::runtime::native_image::host_tvm_target()?;
    let adapter = crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract::current()
        .render_metadata(&target.triple, &target.calling_convention)?;
    let mut metadata = format!(
        "[package]\nnamespace = {:?}\nadapter = \"c-abi\"\ncrate = {:?}\n\n[c_metadata]\nschema = {:?}\nproducer = {:?}\nformat = {:?}\nabi_version = {}\n\n[public_adapter]\n{}raw_pointers_public = false\nraw_pointers_contained_in_adapter = true\nexceptions_cross_boundary = false\n\n",
        manifest.package.namespace,
        manifest.package.crate_name,
        manifest.c_metadata.schema,
        manifest.c_metadata.producer.name,
        manifest.c_metadata.producer.format,
        manifest.c_metadata.abi_version,
        adapter
    );
    for module in &manifest.modules {
        for function in &module.functions {
            metadata.push_str(&format!(
                "[functions.{:?}]\noperation = {:?}\narity = {}\nreturns = {:?}\nblocking = {:?}\nresource = {:?}\n\n",
                format!("{}.{}", module.module, function.name),
                function.operation,
                function.args.len(),
                function.returns,
                blocking_policy_name(&function.blocking),
                resource_policy_name(&function.resource)
            ));
        }
    }
    metadata.truncate(metadata.trim_end().len());
    metadata.push('\n');
    Ok(metadata)
}

pub(super) fn render_rust_adapter_cargo(manifest: &CAbiBindingManifest) -> String {
    let package_policy = if manifest.package.workspace_member {
        "version.workspace = true\nedition.workspace = true\nrust-version.workspace = true\nlicense.workspace = true\nauthors.workspace = true\nrepository.workspace = true"
    } else {
        "version = \"0.0.0\"\nedition = \"2021\""
    };
    let lint_policy = if manifest.package.workspace_member {
        "[lints]\nworkspace = true"
    } else {
        "[lints.rust]\nwarnings = \"deny\""
    };
    let dependency = |name: &str, standalone: &str| {
        if manifest.package.workspace_member {
            format!("{name}.workspace = true")
        } else {
            format!("{name} = {standalone:?}")
        }
    };
    let mut cargo = format!(
        "[package]\nname = {:?}\n{package_policy}\npublish = false\nbuild = \"build.rs\"\n\n{lint_policy}\n\n[lib]\npath = \"src/lib.rs\"\n\n[[bin]]\nname = \"native-boundary-helper\"\npath = \"src/bin/native_boundary_helper.rs\"\n\n[dependencies]\n{}\n{}\n",
        manifest.package.crate_name,
        dependency("base64", "0.22.1"),
        dependency("getrandom", &format!("={GETRANDOM_VERSION}"))
    );
    let uses_pkg_config = manifest
        .c_metadata
        .external_link
        .as_ref()
        .and_then(|link| link.pkg_config.as_ref())
        .is_some();
    if let Some(extension) = &manifest.package.rust_extension {
        for (name, version) in &extension.dependencies {
            cargo.push_str(&format!("{}\n", dependency(name, version)));
        }
    }
    if !manifest.c_metadata.sources.is_empty() || uses_pkg_config {
        cargo.push_str("\n[build-dependencies]\n");
        if !manifest.c_metadata.sources.is_empty() {
            cargo.push_str(&format!(
                "{}\n",
                dependency("cc", &format!("={CC_VERSION}"))
            ));
        }
        if uses_pkg_config {
            cargo.push_str(&format!("{}\n", dependency("pkg-config", "=0.3.33")));
        }
    }
    if !manifest.package.workspace_member {
        cargo.push_str("\n[workspace]\n");
    }
    cargo
}

pub(super) fn render_c_build(metadata: &CMetadata) -> String {
    let cpp_standard = metadata.cpp_standard.as_deref().unwrap_or("c++17");
    let c_sources = metadata
        .sources
        .iter()
        .filter(|source| !is_cpp_adapter_source(source))
        .collect::<Vec<_>>();
    let cpp_sources = metadata
        .sources
        .iter()
        .filter(|source| is_cpp_adapter_source(source))
        .collect::<Vec<_>>();
    let mut build = if metadata
        .external_link
        .as_ref()
        .and_then(|link| link.root_env.as_ref())
        .is_some()
    {
        String::from("use std::path::PathBuf;\n\nfn main() {\n")
    } else {
        String::from("fn main() {\n")
    };
    if let Some(link) = &metadata.external_link {
        if let Some(pkg_config) = &link.pkg_config {
            build.push_str("    let mut probe = pkg_config::Config::new();\n");
            if pkg_config.static_link {
                build.push_str("    probe.statik(true);\n");
            }
            if let Some(min_version) = &pkg_config.min_version {
                build.push_str(&format!("    probe.atleast_version({min_version:?});\n"));
            }
            build.push_str(&format!(
                "    let library = probe\n        .probe({:?})\n        .expect({:?});\n",
                pkg_config.package,
                format!(
                    "pkg-config must resolve external C package {}",
                    pkg_config.package
                )
            ));
            if !c_sources.is_empty() {
                build.push_str("    let mut c_build = cc::Build::new();\n");
                for source in &c_sources {
                    build.push_str(&format!(
                        "    c_build.file({:?});\n",
                        format!(
                            "c/{}",
                            Path::new(source)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                        )
                    ));
                }
                build.push_str(
                    "    for include_path in &library.include_paths {\n        c_build.include(include_path);\n    }\n",
                );
                for directory in &link.include_dirs {
                    build.push_str(&format!("    c_build.include({directory:?});\n"));
                }
                build.push_str(
                    "    c_build\n        .include(\"include\")\n        .include(\".\")\n        .warnings_into_errors(true)\n        .flag_if_supported(\"-std=c11\")\n        .compile(\"terlan_native_boundary_c_abi\");\n",
                );
            }
            if !metadata.sources.is_empty() {
                build.push_str(
                    "    println!(\"cargo:rerun-if-changed=include\");\n    println!(\"cargo:rerun-if-changed=c\");\n",
                );
            }
            build.push_str("}\n");
            return build;
        }
        let root_env = link
            .root_env
            .as_deref()
            .expect("validated environment-rooted external link");
        build.push_str(&format!(
            "    println!(\"cargo:rerun-if-env-changed={}\");\n    let root = PathBuf::from(std::env::var_os({:?}).expect({:?}));\n",
            root_env,
            root_env,
            format!("{root_env} must point at the external C distribution")
        ));
        if !c_sources.is_empty() {
            build.push_str("    let mut c_build = cc::Build::new();\n");
            for source in &c_sources {
                build.push_str(&format!(
                    "    c_build.file({:?});\n",
                    format!(
                        "c/{}",
                        Path::new(source)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                ));
            }
            for directory in &link.include_dirs {
                build.push_str(&format!(
                    "    c_build.include(root.join({:?}));\n",
                    directory
                ));
            }
            build.push_str(
                "    c_build.include(\"include\").include(\".\").warnings_into_errors(true).flag_if_supported(\"-std=c11\").compile(\"terlan_native_boundary_c_abi\");\n",
            );
        }
        if !cpp_sources.is_empty() {
            build.push_str("    let mut cpp_build = cc::Build::new();\n    cpp_build.cpp(true);\n");
            for source in &cpp_sources {
                build.push_str(&format!(
                    "    cpp_build.file({:?});\n",
                    format!(
                        "c/{}",
                        Path::new(source)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                ));
            }
            for directory in &link.include_dirs {
                build.push_str(&format!(
                    "    cpp_build.include(root.join({:?}));\n",
                    directory
                ));
            }
            build.push_str(&format!(
                "    cpp_build.include(\"include\").include(\".\").warnings_into_errors(true).flag_if_supported(\"-std={cpp_standard}\").compile(\"terlan_native_boundary_cpp_abi\");\n"
            ));
        }
        if !metadata.sources.is_empty() {
            build.push_str(
                "    println!(\"cargo:rerun-if-changed=include\");\n    println!(\"cargo:rerun-if-changed=c\");\n",
            );
        }
        for directory in &link.library_dirs {
            build.push_str(&format!(
                "    println!(\"cargo:rustc-link-search=native={{}}\", root.join({:?}).display());\n",
                directory
            ));
        }
        for library in &link.libraries {
            build.push_str(&format!(
                "    println!(\"cargo:rustc-link-lib=dylib={}\");\n",
                library
            ));
        }
        for directory in &link.runtime_library_dirs {
            build.push_str(&format!(
                "    if std::env::var_os(\"CARGO_CFG_TARGET_OS\").as_deref() == Some(std::ffi::OsStr::new(\"linux\")) {{\n        println!(\"cargo:rustc-link-arg=-Wl,-rpath,{{}}\", root.join({:?}).display());\n    }}\n",
                directory
            ));
        }
        build.push_str("}\n");
        return build;
    }

    if !c_sources.is_empty() {
        build.push_str("    let mut c_build = cc::Build::new();\n");
        for source in &c_sources {
            build.push_str(&format!(
                "    c_build.file({:?});\n",
                format!(
                    "c/{}",
                    Path::new(source)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            ));
        }
        build.push_str(
            "    c_build.include(\"include\").include(\".\").warnings_into_errors(true).flag_if_supported(\"-std=c11\").compile(\"terlan_native_boundary_c_abi\");\n",
        );
    }
    if !cpp_sources.is_empty() {
        build.push_str("    let mut cpp_build = cc::Build::new();\n    cpp_build.cpp(true);\n");
        for source in &cpp_sources {
            build.push_str(&format!(
                "    cpp_build.file({:?});\n",
                format!(
                    "c/{}",
                    Path::new(source)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            ));
        }
        build.push_str(&format!(
            "    cpp_build.include(\"include\").include(\".\").warnings_into_errors(true).flag_if_supported(\"-std={cpp_standard}\").compile(\"terlan_native_boundary_cpp_abi\");\n"
        ));
    }
    build.push_str(
        "    println!(\"cargo:rerun-if-changed=include\");\n    println!(\"cargo:rerun-if-changed=c\");\n}\n",
    );
    build
}

pub(super) fn is_cpp_adapter_source(source: &str) -> bool {
    Path::new(source)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "cc" | "cpp" | "cxx" | "C"))
}

pub(super) struct RenderedRustAdapter {
    pub(super) root: String,
    pub(super) chunks: Vec<String>,
}

pub(super) fn render_rust_ffi_and_adapter(
    manifest: &CAbiBindingManifest,
    symbols: &BTreeMap<&str, &CSymbol>,
) -> Result<RenderedRustAdapter, String> {
    let types = binding_types(manifest);
    let functions = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let wrapper_symbols = functions
        .iter()
        .map(|function| function_symbol(function, symbols))
        .collect::<Result<Vec<_>, String>>()?;
    let uses_non_null = !types.is_empty()
        || functions
            .iter()
            .any(|function| function.dispatcher.is_some())
        || wrapper_symbols.iter().any(|symbol| {
            symbol.parameters.iter().any(|parameter| {
                parameter.direction == Some(CParameterDirection::Output)
                    && (parameter.ownership == Some(CParameterOwnership::TransferFull)
                        || parameter.borrowed_array.is_some())
            })
        });
    let uses_status_check = wrapper_symbols
        .iter()
        .any(|symbol| symbol.error_model == Some(CErrorModel::StatusCode));
    let dispatcher_ty = types.first().map(|(_, ty)| *ty);
    let dispatcher_record = dispatcher_ty
        .map(|ty| {
            symbols
                .get(ty.c_symbol.as_str())
                .copied()
                .ok_or_else(|| format!("unknown C record `{}`", ty.c_symbol))
        })
        .transpose()?;

    let mut source = String::from("#![deny(unsafe_op_in_unsafe_fn)]\n\n");
    source.push_str("pub mod ffi {\n");
    let mut adapter_chunks = Vec::new();
    for (_, ty) in &types {
        let record = symbols
            .get(ty.c_symbol.as_str())
            .copied()
            .ok_or_else(|| format!("unknown C record `{}`", ty.c_symbol))?;
        source.push_str(&format!(
            "    #[repr(C)]\n    pub struct {} {{\n        _private: [u8; 0],\n    }}\n\n",
            record.c_name
        ));
    }
    source.push_str("    unsafe extern \"C\" {\n");
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::Function
    }) {
        source.push_str(&render_raw_ffi_function(
            symbol,
            &manifest.c_metadata.aliases,
        )?);
    }
    source.push_str("    }\n}\n\n");
    if uses_non_null {
        source.push_str("use std::ptr::NonNull;\n\n");
    }
    source.push_str(
        "#[derive(Debug, Clone, PartialEq, Eq)]\npub struct CAbiError {\n    pub operation: &'static str,\n    pub status: i32,\n}\n\n",
    );
    for (_, ty) in &types {
        let record = symbols
            .get(ty.c_symbol.as_str())
            .copied()
            .ok_or_else(|| format!("unknown C record `{}`", ty.c_symbol))?;
        source.push_str(&format!(
            "pub struct {} {{\n    raw: NonNull<ffi::{}>,\n}}\n\n",
            ty.name, record.c_name
        ));
        if record.thread_safety.as_deref() == Some("send_only") {
            source.push_str(&format!(
                "// SAFETY: reviewed `send_only` metadata permits exclusive ownership transfer; the wrapper intentionally remains !Sync.\nunsafe impl Send for {} {{}}\n\n",
                ty.name
            ));
        }
    }
    let has_dispatcher = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.dispatcher.is_some());
    if has_dispatcher {
        let dispatcher_ty = dispatcher_ty.ok_or_else(|| {
            "error[native_bindgen.dispatcher_resource_required]: dispatcher package requires an opaque resource".to_string()
        })?;
        let dispatcher_record = dispatcher_record.expect("dispatcher type has a reviewed record");
        let dispatcher_dispose = dispose_for_type(manifest, &dispatcher_ty.name)?;
        let dispatcher_dispose_symbol = function_symbol(dispatcher_dispose, symbols)?;
        source.push_str(&format!(
            "struct DispatcherInputGuard {{\n    raw: Option<NonNull<ffi::{}>>,\n}}\n\nimpl DispatcherInputGuard {{\n    fn new(raw: *mut ffi::{}) -> Option<Self> {{\n        NonNull::new(raw).map(|raw| Self {{ raw: Some(raw) }})\n    }}\n\n    fn into_stable_ivalue(mut self) -> u64 {{\n        self.raw.take().expect(\"dispatcher input guard is armed\").as_ptr() as usize as u64\n    }}\n}}\n\nimpl Drop for DispatcherInputGuard {{\n    fn drop(&mut self) {{\n        if let Some(raw) = self.raw.take() {{\n            // SAFETY: an armed guard exclusively owns a duplicated handle not yet transferred to a dispatcher stack.\n            let status = unsafe {{ ffi::{}(raw.as_ptr()) }};\n            debug_assert_eq!(status, {});\n        }}\n    }}\n}}\n\n",
            dispatcher_record.c_name,
            dispatcher_record.c_name,
            dispatcher_dispose_symbol.c_name,
            dispatcher_dispose_symbol.success_code.unwrap_or(0)
        ));
    }
    if manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter_map(|function| function.dispatcher.as_ref())
        .flat_map(|dispatcher| &dispatcher.stack)
        .any(|value| {
            matches!(
                value,
                CDispatcherStackValue::OwnedOptionalIntArgument { .. }
                    | CDispatcherStackValue::OwnedOptionalHandleCopy { .. }
                    | CDispatcherStackValue::OwnedOptionalIntListArgument { .. }
            )
        })
    {
        source.push_str(
            "struct DispatcherOptionalValueGuard {\n    raw: Option<NonNull<u64>>,\n    destructor: unsafe extern \"C\" fn(*mut u64) -> i32,\n    success: i32,\n}\n\nimpl DispatcherOptionalValueGuard {\n    fn new(\n        raw: *mut u64,\n        destructor: unsafe extern \"C\" fn(*mut u64) -> i32,\n        success: i32,\n    ) -> Option<Self> {\n        NonNull::new(raw).map(|raw| Self { raw: Some(raw), destructor, success })\n    }\n\n    fn write_stable_ivalue(&mut self, value: u64) {\n        // SAFETY: the allocator returned exclusive storage for one StableIValue.\n        unsafe { *self.raw.expect(\"dispatcher optional guard is armed\").as_ptr() = value };\n    }\n\n    fn write_i64(&mut self, value: i64) {\n        self.write_stable_ivalue(value as u64);\n    }\n\n    fn into_stable_ivalue(mut self) -> u64 {\n        self.raw.take().expect(\"dispatcher optional guard is armed\").as_ptr() as usize as u64\n    }\n}\n\nimpl Drop for DispatcherOptionalValueGuard {\n    fn drop(&mut self) {\n        if let Some(raw) = self.raw.take() {\n            // SAFETY: an armed guard exclusively owns optional backing storage not yet transferred to the dispatcher.\n            let status = unsafe { (self.destructor)(raw.as_ptr()) };\n            debug_assert_eq!(status, self.success);\n        }\n    }\n}\n\n",
        );
    }
    if manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter_map(|function| function.dispatcher.as_ref())
        .flat_map(|dispatcher| &dispatcher.stack)
        .any(|value| {
            matches!(
                value,
                CDispatcherStackValue::OwnedIntListArgument { .. }
                    | CDispatcherStackValue::OwnedOptionalIntListArgument { .. }
            )
        })
    {
        source.push_str(
            "struct DispatcherListGuard {\n    raw: Option<NonNull<()>>,\n    destructor: unsafe extern \"C\" fn(*mut ()) -> i32,\n    success: i32,\n}\n\nimpl DispatcherListGuard {\n    fn new(\n        raw: *mut (),\n        destructor: unsafe extern \"C\" fn(*mut ()) -> i32,\n        success: i32,\n    ) -> Option<Self> {\n        NonNull::new(raw).map(|raw| Self { raw: Some(raw), destructor, success })\n    }\n\n    fn as_ptr(&self) -> *mut () {\n        self.raw.expect(\"dispatcher list guard is armed\").as_ptr()\n    }\n\n    fn into_stable_ivalue(mut self) -> u64 {\n        self.raw.take().expect(\"dispatcher list guard is armed\").as_ptr() as usize as u64\n    }\n}\n\nimpl Drop for DispatcherListGuard {\n    fn drop(&mut self) {\n        if let Some(raw) = self.raw.take() {\n            // SAFETY: an armed guard exclusively owns a dispatcher list not yet transferred to the stack.\n            let status = unsafe { (self.destructor)(raw.as_ptr()) };\n            debug_assert_eq!(status, self.success);\n        }\n    }\n}\n\n",
        );
    }
    if manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter_map(|function| function.dispatcher.as_ref())
        .flat_map(|dispatcher| &dispatcher.stack)
        .any(|value| matches!(value, CDispatcherStackValue::OwnedStringLiteral { .. }))
    {
        source.push_str(
            "struct DispatcherStringGuard {\n    raw: Option<NonNull<()>>,\n    destructor: unsafe extern \"C\" fn(*mut ()) -> i32,\n    success: i32,\n}\n\nimpl DispatcherStringGuard {\n    fn new(\n        raw: *mut (),\n        destructor: unsafe extern \"C\" fn(*mut ()) -> i32,\n        success: i32,\n    ) -> Option<Self> {\n        NonNull::new(raw).map(|raw| Self { raw: Some(raw), destructor, success })\n    }\n\n    fn into_stable_ivalue(mut self) -> u64 {\n        self.raw.take().expect(\"dispatcher string guard is armed\").as_ptr() as usize as u64\n    }\n}\n\nimpl Drop for DispatcherStringGuard {\n    fn drop(&mut self) {\n        if let Some(raw) = self.raw.take() {\n            // SAFETY: an armed guard exclusively owns a dispatcher string not yet transferred to the stack.\n            let status = unsafe { (self.destructor)(raw.as_ptr()) };\n            debug_assert_eq!(status, self.success);\n        }\n    }\n}\n\n",
        );
    }
    for (_, ty) in &types {
        let record = symbols
            .get(ty.c_symbol.as_str())
            .copied()
            .ok_or_else(|| format!("unknown C record `{}`", ty.c_symbol))?;
        let dispose = dispose_for_type(manifest, &ty.name)?;
        let dispose_symbol = function_symbol(dispose, symbols)?;
        let owned_functions = manifest
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                matches!(
                    function.role,
                    CAbiFunctionRole::Constructor
                        | CAbiFunctionRole::ImmutableMethod
                        | CAbiFunctionRole::MutableMethod
                ) && function_owner_type(manifest, function)
                    .is_ok_and(|owner| owner.name == ty.name)
            })
            .collect::<Vec<_>>();
        if owned_functions.len() <= 64 {
            source.push_str(&format!("impl {} {{\n", ty.name));
            for function in owned_functions {
                source.push_str(&render_safe_wrapper(
                    SafeWrapperRendering {
                        manifest,
                        symbols,
                        aliases: &manifest.c_metadata.aliases,
                    },
                    SafeWrapperTarget {
                        function,
                        symbol: function_symbol(function, symbols)?,
                        record: Some(record),
                        ty: Some(ty),
                        inside_impl: true,
                    },
                )?);
            }
            source.push_str("}\n\n");
        } else {
            for functions in owned_functions.chunks(32) {
                let chunk_index = adapter_chunks.len();
                let mut chunk = format!("use super::*;\n\nimpl {} {{\n", ty.name);
                for function in functions {
                    chunk.push_str(&render_safe_wrapper(
                        SafeWrapperRendering {
                            manifest,
                            symbols,
                            aliases: &manifest.c_metadata.aliases,
                        },
                        SafeWrapperTarget {
                            function,
                            symbol: function_symbol(function, symbols)?,
                            record: Some(record),
                            ty: Some(ty),
                            inside_impl: true,
                        },
                    )?);
                }
                chunk.push_str("}\n");
                adapter_chunks.push(chunk);
                source.push_str(&format!("mod generated_adapter_{chunk_index};\n"));
            }
            source.push('\n');
        }
        source.push_str(&format!(
            "impl Drop for {} {{\n    fn drop(&mut self) {{\n        // SAFETY: this adapter is the sole owner and invokes the destructor once.\n        let status = unsafe {{ ffi::{}(self.raw.as_ptr()) }};\n        debug_assert_eq!(status, {});\n    }}\n}}\n\n",
            ty.name,
            dispose_symbol.c_name,
            dispose_symbol.success_code.unwrap_or(0)
        ));
    }
    for function in manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == CAbiFunctionRole::FreeFunction)
    {
        let rendered = render_safe_wrapper(
            SafeWrapperRendering {
                manifest,
                symbols,
                aliases: &manifest.c_metadata.aliases,
            },
            SafeWrapperTarget {
                function,
                symbol: function_symbol(function, symbols)?,
                record: dispatcher_record,
                ty: dispatcher_ty,
                inside_impl: false,
            },
        )?;
        source.push_str(&rendered);
    }
    if uses_status_check {
        source.push_str(
            "fn check_status<T>(operation: &'static str, status: T, success: i32) -> Result<(), CAbiError>\nwhere\n    T: TryInto<i32>,\n{\n    let status = status.try_into().map_err(|_| CAbiError { operation, status: -2 })?;\n    if status == success {\n        Ok(())\n    } else {\n        Err(CAbiError { operation, status })\n    }\n}\n\n",
        );
    }
    if manifest.package.rust_extension.is_some() {
        source.push_str("mod package_extension;\npub use package_extension::*;\n");
    }
    Ok(RenderedRustAdapter {
        root: source,
        chunks: adapter_chunks,
    })
}
