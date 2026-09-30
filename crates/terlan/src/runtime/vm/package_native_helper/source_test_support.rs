//! Compile actual source modules and verify the in-process package boundary.

use super::*;
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};

pub(super) fn assert_source_checks(module: &str, contents: &str, entries: &[&str]) {
    let directory = TestDirectory::new("package-source", module);
    let source = directory.join(&format!("{module}.terl"));
    std::fs::write(&source, contents).unwrap();
    let output = directory.join("build");
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![source.display().to_string()],
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            }
        ),
        std::process::ExitCode::SUCCESS
    );
    let mut shard =
        PureNativeExecutionShard::load_image(&output.join(format!("vm/{module}.tvm"))).unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    for entry in entries {
        assert_eq!(
            execute_call(&mut shard, &mut helpers, entry, &[]).unwrap(),
            ReplValue::Bool(true),
            "{module}.{entry}"
        );
    }
    assert!(
        helpers.helpers.is_empty(),
        "copied-value calls must not spawn native helpers"
    );
    assert!(helpers.helper_paths.is_empty());
    drop(helpers);
    drop(shard);
    directory.close();
}
