//! Serve images keep HTTP entrypoints when a project also has a CLI main.

use super::*;
use crate::CliCommand;
use std::process::ExitCode;

#[test]
fn generated_web_project_keeps_http_roots_beside_cli_main() {
    let temporary = crate::support::test_fs::temp_path("serve", "http_and_cli_roots");
    let project = temporary.join("serve_roots");
    fs::create_dir_all(&temporary).expect("create fixture root");
    assert_eq!(
        crate::commands::init::run(CliCommand {
            verb: Some("init".into()),
            args: vec![
                project.display().to_string(),
                "--profile".into(),
                "web".into()
            ],
        },),
        ExitCode::SUCCESS,
    );
    let web_root = project.join("_build/web");
    fs::create_dir_all(&web_root).expect("create web output");
    let compiled = compile_serve_application(
        &web_root,
        &project.join("src/serve_roots/Http.terl"),
        "serve_roots.Http",
    )
    .expect("compile HTTP image independently of the CLI main");
    assert!(compiled.image.path.is_file());
    fs::remove_dir_all(temporary).expect("clean fixture");
}
