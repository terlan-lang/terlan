//! Startup for directly executable Terlan applications.

use crate::runtime::executable::{ApplicationEntry, EmbeddedApplication, ROLE_ENV, WORKER_ROLE};

use super::*;

pub(super) fn run(application: &mut EmbeddedApplication) -> Result<ExitCode, String> {
    let entry = application.entry().clone();
    if entry == ApplicationEntry::Worker {
        return Ok(crate::native_worker::run_from_env());
    }
    let files = application.materialize()?;
    application.pin_for_workers(&files)?;
    // Application startup runs before VM owners or service threads are created.
    for binding in application.environment() {
        std::env::set_var(&binding.name, files.root().join(&binding.path));
    }
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match entry {
        ApplicationEntry::Worker => unreachable!("worker entry handled before application startup"),
        ApplicationEntry::Cli { image, function } => {
            run_tvm_image(
                &files.root().join(image),
                &function,
                RunResultMode::Discard,
                &arguments,
                &[],
                &mut |line| println!("{line}"),
            )?;
            Ok(ExitCode::SUCCESS)
        }
        ApplicationEntry::Service { web_root } => {
            if std::env::var_os("TERLAN_SERVE_PORT").is_none() {
                if let Some(port) = std::env::var_os("PORT") {
                    std::env::set_var("TERLAN_SERVE_PORT", port);
                }
            }
            if std::env::var_os("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").is_none() {
                std::env::set_var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES", "1");
            }
            std::env::set_var("TERLAN_SERVE_MANIFEST_ONLY", "1");
            let mut args = vec![files.root().join(web_root).to_string_lossy().into_owned()];
            args.extend(arguments);
            Ok(crate::run_serve_runtime(args))
        }
    }
}

pub(super) fn worker_role() -> bool {
    std::env::var(ROLE_ENV).as_deref() == Ok(WORKER_ROLE)
}
