use super::*;

/// Links one Cranelift application object and descriptor object exactly once.
pub(super) fn link_native_image(
    unit_paths: &[PathBuf],
    object_path: &Path,
    descriptor_object_path: &Path,
    image_path: &Path,
    linker_policy: &NativeLinkerPolicy,
    policy: NativeCodegenPolicy,
    input_sha256: &str,
) -> Result<(), BuildOneError> {
    let mut command = Command::new(&linker_policy.program);
    if cfg!(target_os = "macos") {
        command
            .arg("-dynamiclib")
            .arg("-Wl,-undefined,dynamic_lookup")
            .arg("-o")
            .arg(image_path);
        if policy.optimizes_link() {
            command.arg("-Wl,-dead_strip");
        }
        command
            .args(unit_paths)
            .arg(object_path)
            .arg(descriptor_object_path);
    } else if cfg!(target_os = "windows") {
        if linker_policy.bundled_windows_linker {
            command.arg("-flavor").arg("link");
        }
        command
            .arg("/DLL")
            .arg(format!("/ENTRY:{IMAGE_ENTRY_SYMBOL}"))
            .arg(format!("/EXPORT:{DISPATCH_SYMBOL}"))
            .arg(format!("/OUT:{}", image_path.display()));
        if policy.optimizes_link() {
            command.arg("/OPT:REF").arg("/OPT:ICF");
        }
        command
            .args(unit_paths)
            .arg(object_path)
            .arg(descriptor_object_path);
    } else {
        command
            .arg("-shared")
            .arg("-e")
            .arg(IMAGE_ENTRY_SYMBOL)
            .arg("-z")
            .arg("noexecstack")
            .arg("-o")
            .arg(image_path);
        if policy.optimizes_link() {
            command.arg("-O1");
        }
        command
            .args(unit_paths)
            .arg(object_path)
            .arg(descriptor_object_path);
    }
    let label = format!(
        "native linker `{}` for `{}`",
        Path::new(&linker_policy.program).display(),
        image_path.display()
    );
    let mut activity = Activity::begin(Operation::NativeLink, input_sha256)?;
    let output = capture_tool_command_with_launch(
        &mut command,
        &label,
        Duration::from_secs(300),
        16 * 1024 * 1024,
        |pid| activity.spawned(pid).map_err(|error| format!("{error:?}")),
    )
    .map_err(|error| BuildOneError::Message(format!("error[tvm.native_link.execution]: {error}")));
    activity.finish(
        output.as_ref().is_ok_and(|output| output.status.success()),
        None,
    )?;
    let output = output?;
    if !output.status.success() {
        return Err(BuildOneError::Message(format!(
            "native linker failed for `{}`:\n{}",
            image_path.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

/// Resolves and fingerprints the exact linker admitted by this compiler process.
///
/// A linker override is a code-generation input: accepting an image produced by
/// a different linker would turn a warm cache hit into untracked environment
/// dependence. The binary digest is computed once per compiler process.
pub(super) fn native_linker_policy() -> Result<&'static NativeLinkerPolicy, BuildOneError> {
    NATIVE_LINKER_POLICY
        .get_or_init(|| {
            let (program, bundled_windows_linker) =
                if let Some(linker) = std::env::var_os("TERLAN_NATIVE_LINKER") {
                    (linker, false)
                } else {
                    let (linker, bundled) = default_native_linker().map_err(build_error_message)?;
                    (linker, bundled)
                };
            let resolved = resolve_linker_program(&program).ok_or_else(|| {
                format!(
                    "error[tvm.cache.linker_identity]: cannot resolve native linker `{}`",
                    Path::new(&program).display()
                )
            })?;
            let bytes = fs::read(&resolved).map_err(|error| {
                format!(
                    "error[tvm.cache.linker_identity]: cannot read native linker `{}`: {error}",
                    resolved.display()
                )
            })?;
            let cache_identity = native_cache::sha256_hex(
                format!(
                    "terlan-native-linker-v2\0{}\0{}\0{}",
                    bundled_windows_linker,
                    native_cache::sha256_hex(&bytes),
                    super::super::linker_identity::environment_digest(),
                )
                .as_bytes(),
            );
            Ok(NativeLinkerPolicy {
                program: resolved.into_os_string(),
                bundled_windows_linker,
                cache_identity,
            })
        })
        .as_ref()
        .map_err(|error| BuildOneError::Message(error.clone()))
}

pub(super) fn resolve_linker_program(program: &OsStr) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return fs::canonicalize(path)
            .ok()
            .filter(|candidate| candidate.is_file());
    }
    let search_path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&search_path) {
        let candidate = directory.join(path);
        if let Ok(resolved) = fs::canonicalize(&candidate) {
            if resolved.is_file() {
                return Some(resolved);
            }
        }
        #[cfg(windows)]
        if candidate.extension().is_none() {
            let executable = candidate.with_extension("exe");
            if let Ok(resolved) = fs::canonicalize(&executable) {
                if resolved.is_file() {
                    return Some(resolved);
                }
            }
        }
    }
    None
}

/// Resolves the host linker without relying on ambiguous platform `PATH`
/// entries.
pub(super) fn default_native_linker() -> Result<(OsString, bool), BuildOneError> {
    #[cfg(target_os = "windows")]
    {
        bundled_rust_lld().map(|linker| (linker, true))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(("cc".into(), false))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(("ld".into(), false))
    }
}

/// Finds the COFF-capable linker shipped with the active Rust toolchain.
#[cfg(target_os = "windows")]
pub(super) fn bundled_rust_lld() -> Result<OsString, BuildOneError> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = capture_tool_command(
        Command::new(&rustc).args(["--print", "target-libdir"]),
        "Rust target library directory probe",
        Duration::from_secs(30),
        1024 * 1024,
    )
    .map_err(|error| {
        BuildOneError::Message(format!(
            "failed to locate the Rust target library directory with `{}`: {error}",
            Path::new(&rustc).display()
        ))
    })?;
    if !output.status.success() {
        return Err(BuildOneError::Message(format!(
            "`{}` could not locate the Rust target library directory:\n{}",
            Path::new(&rustc).display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let target_libdir = String::from_utf8(output.stdout)
        .map_err(|_| {
            BuildOneError::Message("Rust target library directory is not valid UTF-8".to_string())
        })?
        .trim()
        .to_string();
    let target_root = Path::new(&target_libdir).parent().ok_or_else(|| {
        BuildOneError::Message(format!(
            "Rust target library directory `{target_libdir}` has no toolchain root"
        ))
    })?;
    let linker = target_root.join("bin").join("rust-lld.exe");
    if !linker.is_file() {
        return Err(BuildOneError::Message(format!(
            "Rust toolchain linker `{}` does not exist",
            linker.display()
        )));
    }
    Ok(linker.into_os_string())
}
