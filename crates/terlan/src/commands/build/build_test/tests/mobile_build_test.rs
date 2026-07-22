use super::*;
use serde_json::Value;

/// Verifies `terlc build --target mobile.android` emits a planning manifest.
///
/// Inputs:
/// - Build command selecting the Android mobile target.
/// - Dedicated build output directory.
///
/// Output:
/// - `_build/mobile/android/plan.json` containing the mobile planning contract.
///
/// Transformation:
/// - Runs the build command through the mobile planner without invoking Vm,
///   JS bundling, Gradle, or Apple tooling.
#[test]
fn build_command_mobile_android_emits_planning_manifest() {
    let dir = make_temp_dir("mobile_android_plan");
    let out_dir = dir.join("_build");
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.android"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let plan_path = out_dir.join("mobile/android/plan.json");
    let plan: Value =
        serde_json::from_str(&fs::read_to_string(plan_path).expect("read mobile plan"))
            .expect("parse mobile plan");

    assert_eq!(plan["schema"], "terlan-mobile-build-plan-v1");
    assert_eq!(plan["target"], "mobile.android");
    assert_eq!(plan["platform"], "android");
    assert_eq!(plan["status"], "planning_metadata_available");
    assert_eq!(
        plan["diagnostic_code"],
        "mobile_shell_emission_experimental"
    );
    assert_eq!(
        plan["diagnostic_message"],
        "mobile.android planning metadata exists; full native shell emission remains experimental and incomplete"
    );
    assert_eq!(plan["source_path"], "src");
    assert_eq!(plan["shell_generation"], "planned");
    assert_eq!(plan["native_shell_project"], "shell");
    assert!(plan["web_output"].is_null());
    assert_eq!(plan["route_manifest"], "metadata/routes.json");
    assert_eq!(plan["bridge_manifest"], "metadata/bridge.json");
    assert_eq!(plan["capability_manifest"], "metadata/capabilities.json");
    assert_eq!(plan["native_shell_config"], "metadata/native-shell.json");
    assert_eq!(
        plan["source_identity_metadata"],
        "metadata/source-identities.json"
    );
    assert!(!out_dir.join("js").exists());
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
}

/// Verifies `terlc build --target mobile.ios` emits a planning manifest.
///
/// Inputs:
/// - Build command selecting the iOS mobile target.
/// - Dedicated build output directory.
///
/// Output:
/// - `_build/mobile/ios/plan.json` containing the mobile planning contract.
///
/// Transformation:
/// - Runs the build command through the mobile planner without invoking Vm,
///   JS bundling, Gradle, or Apple tooling.
#[test]
fn build_command_mobile_ios_emits_planning_manifest() {
    let dir = make_temp_dir("mobile_ios_plan");
    let out_dir = dir.join("_build");
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.ios"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let plan_path = out_dir.join("mobile/ios/plan.json");
    let plan: Value =
        serde_json::from_str(&fs::read_to_string(plan_path).expect("read mobile plan"))
            .expect("parse mobile plan");

    assert_eq!(plan["schema"], "terlan-mobile-build-plan-v1");
    assert_eq!(plan["target"], "mobile.ios");
    assert_eq!(plan["platform"], "ios");
    assert_eq!(plan["status"], "planning_metadata_available");
    assert_eq!(
        plan["diagnostic_code"],
        "mobile_shell_emission_experimental"
    );
    assert_eq!(
        plan["diagnostic_message"],
        "mobile.ios planning metadata exists; full native shell emission remains experimental and incomplete"
    );
    assert_eq!(plan["source_path"], "src");
    assert_eq!(plan["shell_generation"], "planned");
    assert_eq!(plan["native_shell_project"], "shell");
    assert!(plan["web_output"].is_null());
    assert!(!out_dir.join("js").exists());
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
}

/// Verifies mobile planning packages existing browser output as shell input.
///
/// Inputs:
/// - Existing `_build/web` browser package fixture.
/// - Android mobile build command.
///
/// Output:
/// - Copied browser package under `_build/mobile/android/shell-inputs/web`.
/// - Mobile plan pointing at `shell-inputs/web`.
///
/// Transformation:
/// - Treats browser output as an already-produced AngularTS/web shell input and
///   copies it into the native shell build input directory.
#[test]
fn build_command_mobile_android_packages_existing_web_output() {
    let dir = make_temp_dir("mobile_android_web_input");
    let out_dir = dir.join("_build");
    let web_dir = out_dir.join("web");
    fs::create_dir_all(web_dir.join("assets")).expect("create web fixture");
    fs::write(
        web_dir.join("manifest.json"),
        "{\"schema\":\"terlan-web-build-v1\"}",
    )
    .expect("write manifest");
    fs::write(web_dir.join("assets/app.js"), "console.log('terlan');").expect("write asset");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.android"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let plan_path = out_dir.join("mobile/android/plan.json");
    let plan: Value =
        serde_json::from_str(&fs::read_to_string(plan_path).expect("read mobile plan"))
            .expect("parse mobile plan");

    assert_eq!(plan["web_output"], "shell-inputs/web");
    assert_eq!(
        fs::read_to_string(out_dir.join("mobile/android/shell-inputs/web/manifest.json"))
            .expect("read packaged manifest"),
        "{\"schema\":\"terlan-web-build-v1\"}"
    );
    assert_eq!(
        fs::read_to_string(out_dir.join("mobile/android/shell-inputs/web/assets/app.js"))
            .expect("read packaged asset"),
        "console.log('terlan');"
    );
}

/// Verifies iOS mobile planning uses the same browser shell input packaging.
///
/// Inputs:
/// - Existing `_build/web` browser package fixture.
/// - iOS mobile build command.
///
/// Output:
/// - Copied browser package under `_build/mobile/ios/shell-inputs/web`.
///
/// Transformation:
/// - Exercises the shared mobile planner path for the iOS platform label.
#[test]
fn build_command_mobile_ios_packages_existing_web_output() {
    let dir = make_temp_dir("mobile_ios_web_input");
    let out_dir = dir.join("_build");
    let web_dir = out_dir.join("web");
    fs::create_dir_all(&web_dir).expect("create web fixture");
    fs::write(
        web_dir.join("manifest.json"),
        "{\"schema\":\"terlan-web-build-v1\"}",
    )
    .expect("write manifest");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.ios"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let plan_path = out_dir.join("mobile/ios/plan.json");
    let plan: Value =
        serde_json::from_str(&fs::read_to_string(plan_path).expect("read mobile plan"))
            .expect("parse mobile plan");

    assert_eq!(plan["web_output"], "shell-inputs/web");
    assert!(out_dir
        .join("mobile/ios/shell-inputs/web/manifest.json")
        .exists());
}

/// Verifies mobile planning emits the shell metadata files named by the plan.
///
/// Inputs:
/// - Android mobile build command.
///
/// Output:
/// - Route, bridge, native shell config, and source identity metadata under
///   the platform-specific mobile metadata directory.
///
/// Transformation:
/// - Emits first-slice empty metadata from compiler-owned mobile contracts so
///   future source collection can fill the same files.
#[test]
fn build_command_mobile_android_emits_shell_metadata_files() {
    let dir = make_temp_dir("mobile_android_shell_metadata");
    let out_dir = dir.join("_build");
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.android"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let metadata_dir = out_dir.join("mobile/android/metadata");
    let routes: Value =
        serde_json::from_str(&fs::read_to_string(metadata_dir.join("routes.json")).unwrap())
            .expect("parse routes");
    let bridge: Value =
        serde_json::from_str(&fs::read_to_string(metadata_dir.join("bridge.json")).unwrap())
            .expect("parse bridge");
    let capabilities: Value =
        serde_json::from_str(&fs::read_to_string(metadata_dir.join("capabilities.json")).unwrap())
            .expect("parse capabilities");
    let shell: Value =
        serde_json::from_str(&fs::read_to_string(metadata_dir.join("native-shell.json")).unwrap())
            .expect("parse native shell");
    let identities: Value = serde_json::from_str(
        &fs::read_to_string(metadata_dir.join("source-identities.json")).unwrap(),
    )
    .expect("parse source identities");

    assert_eq!(routes["schema_version"], 1);
    assert!(routes["routes"].as_array().unwrap().is_empty());
    assert_eq!(bridge["schema_version"], 1);
    assert!(bridge["declarations"].as_array().unwrap().is_empty());
    assert_eq!(capabilities["schema_version"], 1);
    assert!(capabilities["resources"].as_array().unwrap().is_empty());
    assert_eq!(shell["schema"], "terlan-mobile-native-shell-config-v1");
    assert_eq!(shell["target"], "mobile.android");
    assert_eq!(shell["platform"], "android");
    assert_eq!(shell["native_shell_project"], "shell");
    assert_eq!(shell["route_manifest"], "metadata/routes.json");
    assert_eq!(shell["bridge_manifest"], "metadata/bridge.json");
    assert_eq!(shell["capability_manifest"], "metadata/capabilities.json");
    assert_eq!(
        shell["source_identity_metadata"],
        "metadata/source-identities.json"
    );
    assert_eq!(identities["schema"], "terlan-mobile-source-identities-v1");
    assert!(identities["identities"].as_array().unwrap().is_empty());
}

/// Verifies Android mobile planning writes a native shell project skeleton.
///
/// Inputs:
/// - Android mobile build command.
///
/// Output:
/// - Stable Android shell files under `_build/mobile/android/shell`.
///
/// Transformation:
/// - Uses the compiler-owned Android shell layout generator through the build
///   planner and checks representative generated files.
#[test]
fn build_command_mobile_android_emits_shell_project_layout() {
    let dir = make_temp_dir("mobile_android_shell_project");
    let out_dir = dir.join("_build");
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.android"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let shell_dir = out_dir.join("mobile/android/shell");
    assert!(shell_dir.join("settings.gradle.kts").exists());
    assert!(shell_dir.join("app/src/main/AndroidManifest.xml").exists());
    assert!(shell_dir
        .join("app/src/main/java/io/terlan/mobile/MainActivity.kt")
        .exists());
    assert!(fs::read_to_string(shell_dir.join("settings.gradle.kts"))
        .expect("read settings")
        .contains("TerlanMobile"));
}

/// Verifies iOS mobile planning writes a native shell project skeleton.
///
/// Inputs:
/// - iOS mobile build command.
///
/// Output:
/// - Stable Swift package files under `_build/mobile/ios/shell`.
///
/// Transformation:
/// - Uses the compiler-owned iOS shell layout generator through the build
///   planner and checks representative generated files.
#[test]
fn build_command_mobile_ios_emits_shell_project_layout() {
    let dir = make_temp_dir("mobile_ios_shell_project");
    let out_dir = dir.join("_build");
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.ios"]),
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let shell_dir = out_dir.join("mobile/ios/shell");
    assert!(shell_dir.join("Package.swift").exists());
    assert!(shell_dir
        .join("Sources/TerlanMobileCore/TerlanMobileBridge.swift")
        .exists());
    assert!(shell_dir
        .join("Sources/TerlanMobileNavigation/TerlanMobileRouter.swift")
        .exists());
    assert!(fs::read_to_string(shell_dir.join("Package.swift"))
        .expect("read package")
        .contains("TerlanMobile"));
}

/// Verifies mobile planning respects global no-emit mode.
///
/// Inputs:
/// - Build command selecting `mobile.android`.
/// - Global `--no-emit` equivalent state.
///
/// Output:
/// - Successful command result without a written mobile plan.
///
/// Transformation:
/// - Exercises command planning without artifact emission so release checks can
///   distinguish validation intent from generated output.
#[test]
fn build_command_mobile_android_no_emit_skips_plan_write() {
    let dir = make_temp_dir("mobile_android_no_emit");
    let out_dir = dir.join("_build");
    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: args(&["src", "--target", "mobile.android"]),
        },
        CliState {
            no_emit: true,
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("mobile/android/plan.json").exists());
}
