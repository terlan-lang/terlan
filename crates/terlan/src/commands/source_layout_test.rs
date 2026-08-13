use super::*;

#[test]
fn module_layout_accepts_matching_source_path() {
    assert_eq!(
        validate_module_layout(Path::new("src"), Path::new("src/app/Main.terl"), "app.Main",),
        Ok(())
    );
}

#[test]
fn module_layout_rejects_filename_module_mismatch() {
    let error = validate_module_layout(
        Path::new("src"),
        Path::new("src/wrong_module_name.terl"),
        "arne",
    )
    .expect_err("mismatched module declaration must fail");

    assert_eq!(
            error,
            "module declaration `arne` does not match source path `src/wrong_module_name.terl`; expected `module wrong_module_name.`"
        );
}

#[test]
fn module_layout_rejects_nested_module_mismatch() {
    let error = validate_module_layout(
        Path::new("src"),
        Path::new("src/app/Main.terl"),
        "app.Wrong",
    )
    .expect_err("nested module mismatch must fail");

    assert_eq!(
            error,
            "module declaration `app.Wrong` does not match source path `src/app/Main.terl`; expected `module app.Main.`"
        );
}
