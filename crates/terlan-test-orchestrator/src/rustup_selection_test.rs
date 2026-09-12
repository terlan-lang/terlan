use super::*;
use crate::test_orchestrator_test::temporary_fixture;

#[test]
fn active_selection_preserves_named_and_path_toolchains_including_spaces() {
    let fixture = temporary_fixture("rustup-active-name");
    let home = fixture.0.join("rustup");
    let root = home.join("toolchains/custom");
    assert_eq!(
        parse(b"custom (default)\r\n", &root, Some(&home))
            .unwrap()
            .name,
        "custom"
    );
    let absolute = format!(
        "{} (overridden by environment variable RUSTUP_TOOLCHAIN)\n",
        root.display()
    );
    assert_eq!(
        parse(absolute.as_bytes(), &root, Some(&home)).unwrap().name,
        root.to_str().unwrap()
    );
    let root = fixture.0.join("path (with spaces)");
    let absolute = format!(
        "{} (overridden by '/some (source)/rust-toolchain.toml')\n",
        root.display()
    );
    assert_eq!(
        parse(absolute.as_bytes(), &root, None).unwrap().name,
        root.to_str().unwrap()
    );
    for output in [
        b"custom (default)\nextra\n".as_slice(),
        b"custom\n",
        b"\xff",
        b"other (default)\n",
    ] {
        assert!(parse(output, &root, Some(&home)).is_err());
    }
}
