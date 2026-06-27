use super::*;

/// Verifies directory builds compile imported constructor declarations and
/// eligible type-alias constructors.
///
/// Inputs:
/// - A provider module exporting an explicit `Box` constructor and a
///   single-shape `Ok[T]` alias.
/// - A consumer module importing both names and constructing values through
///   the imported constructor-like names.
///
/// Output:
/// - Test passes when `terlc build <dir> --target erlang` emits Erlang source
///   and BEAM artifacts for both modules.
///
/// Transformation:
/// - Runs directory build discovery, import resolution, constructor identity
///   lowering, Erlang source emission, and `erlc`.
#[test]
fn build_command_compiles_directory_with_imported_constructors_and_aliases() {
    let dir = make_temp_dir("directory_imported_constructors");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join("a_user.terl"),
        "module a_user.\n\nimport z_shapes.{Box, Ok}.\n\npub make_box(value: Int): Dynamic ->\n    Box(value).\n\npub make_ok(value: Int): Dynamic ->\n    Ok(value).\n",
    )
    .expect("failed to write constructor user source fixture");
    fs::write(
        source_dir.join("z_shapes.terl"),
        "module z_shapes.\n\npub type Ok[T] =\n    {:ok, value: T}.\n\npub constructor Box {\n    (value: Int): Dynamic ->\n        {:box, value}\n}.\n",
    )
    .expect("failed to write constructor provider source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/a_user.erl").exists());
    assert!(out_dir.join("src/z_shapes.erl").exists());
    assert!(out_dir.join("ebin/a_user.beam").exists());
    assert!(out_dir.join("ebin/z_shapes.beam").exists());
}

/// Verifies directory builds compile aliased imported constructor-like aliases
/// in expression and pattern positions.
///
/// Inputs:
/// - A provider module exporting a single-shape `Ok[T]` alias.
/// - A consumer module importing `Ok as Success`, constructing
///   `Success(value)`, and matching `Success(value)` in a `case`.
///
/// Output:
/// - Test passes when `terlc build <dir> --target erlang` emits Erlang source
///   and BEAM artifacts for both modules.
///
/// Transformation:
/// - Runs the formal directory build path through interface-cache validation,
///   CoreIR lowering, Erlang source emission, and `erlc` so aliased imported
///   alias identities are proven at artifact level.
#[test]
fn build_command_compiles_directory_with_aliased_imported_alias_patterns() {
    let dir = make_temp_dir("directory_aliased_imported_alias_patterns");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join("a_user.terl"),
        "module a_user.\n\nimport z_result.{Ok as Success}.\n\npub make_success(value: Int): Dynamic ->\n    Success(value).\n\npub unwrap_success(input: Dynamic): Dynamic ->\n    case input {\n        Success(value) -> value;\n        _ -> 0\n    }.\n",
    )
    .expect("failed to write aliased alias user source fixture");
    fs::write(
        source_dir.join("z_result.terl"),
        "module z_result.\n\npub type Ok[T] =\n    {:ok, value: T}.\n",
    )
    .expect("failed to write aliased alias provider source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/a_user.erl").exists());
    assert!(out_dir.join("src/z_result.erl").exists());
    assert!(out_dir.join("ebin/a_user.beam").exists());
    assert!(out_dir.join("ebin/z_result.beam").exists());
}

/// Verifies directory builds compile aliased imported constructor-like aliases
/// used as constructor-chain bases.
///
/// Inputs:
/// - A provider module exporting a single-shape `User` alias.
/// - A consumer module importing `User as Member` and using
///   `Member(id, name) with Admin { ... }`.
///
/// Output:
/// - Test passes when `terlc build <dir> --target erlang` emits Erlang source
///   and BEAM artifacts for both modules.
///
/// Transformation:
/// - Runs the formal directory build path through interface-cache validation,
///   constructor-chain identity resolution, CoreIR lowering, Erlang source
///   emission, and `erlc` so aliased imported constructor chains are proven at
///   artifact level.
#[test]
fn build_command_compiles_directory_with_aliased_imported_alias_constructor_chain() {
    let dir = make_temp_dir("directory_aliased_imported_alias_constructor_chain");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join("a_user.terl"),
        "module a_user.\n\nimport z_user.{User as Member}.\n\npub make_admin(id: Int, name: Binary): Dynamic ->\n    Member(id, name) with Admin { id = id, name = name }.\n",
    )
    .expect("failed to write aliased alias constructor-chain user source fixture");
    fs::write(
        source_dir.join("z_user.terl"),
        "module z_user.\n\npub type User =\n    {:user, id: Int, name: Binary}.\n",
    )
    .expect("failed to write aliased alias constructor-chain provider source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(out_dir.join("src/a_user.erl").exists());
    assert!(out_dir.join("src/z_user.erl").exists());
    assert!(out_dir.join("ebin/a_user.beam").exists());
    assert!(out_dir.join("ebin/z_user.beam").exists());
}
