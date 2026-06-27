use super::*;

/// Verifies manifest-backed executable builds require the canonical entrypoint.
///
/// Inputs:
/// - A project manifest selecting the default `beam-thin` artifact.
/// - A package-rooted `app.Main` module that lacks `main/0`.
///
/// Output:
/// - Test passes when the build fails and no user-facing executable
///   launcher or package metadata is written.
///
/// Transformation:
/// - Runs the manifest project build and proves A0.46 checks package
///   entrypoint shape before materializing the runnable artifact contract.
#[test]
fn build_command_rejects_project_manifest_without_main_entrypoint() {
    let dir = make_temp_dir("directory_project_manifest_missing_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write manifest source-root module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("bin/app").exists());
    assert!(!out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists());
}

/// Verifies manifest builds lower explicit constructor declarations.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project.
/// - A package-rooted `app.Main` module with one public constructor and
///   one private constructor used by `main/0`.
///
/// Output:
/// - Test passes when the build emits BEAM artifacts, exports only the
///   public constructor helper, and the generated launcher runs `main/0`.
///
/// Transformation:
/// - Compiles explicit constructor declarations through the formal CoreIR
///   build path and proves constructor visibility controls the emitted
///   public construction API.
#[test]
fn build_command_compiles_project_explicit_constructor_entrypoint() {
    let dir = make_temp_dir("directory_project_explicit_constructor_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
pub type Done = Int.\n\
type Hidden = Int.\n\
\n\
pub constructor Done {\n\
(value: Int): Done -> value\n\
}.\n\
\n\
constructor Hidden {\n\
(value: Int): Hidden -> value\n\
}.\n\
\n\
pub main(): Unit ->\n\
let visible = Done(1); hidden = Hidden(2); std.io.Console.println(\"constructors ok\").\n",
    )
    .expect("failed to write explicit constructor module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("typer_ctor_done_1/1"),
        "public constructor helper should be exported and callable:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("typer_ctor_hidden_1(Value) ->"),
        "private constructor helper should still lower for local use:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("typer_ctor_hidden_0/0"),
        "private constructor helper must not be exported:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run constructor launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "constructors ok\n"
    );
}

/// Verifies manifest builds lower general receiver-method dispatch.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project.
/// - A package-rooted `app.Main` module with a struct, a receiver method,
///   and an executable entrypoint that invokes the method through
///   `receiver.method()`.
///
/// Output:
/// - Test passes when the build emits BEAM artifacts, rewrites the method
///   call to the receiver-first backend convention, and the generated
///   launcher prints the method result.
///
/// Transformation:
/// - Compiles local receiver-method dispatch through the formal
///   syntax-output/typecheck/build path and proves the compatibility
///   Erlang backend can execute the lowered method call.
#[test]
fn build_command_compiles_project_receiver_method_entrypoint() {
    let dir = make_temp_dir("directory_project_receiver_method_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
pub struct User {\n\
name: String\n\
}.\n\
\n\
pub constructor User {\n\
(name: String): User -> User(name = name)\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
user.name.\n\
\n\
show(user: User): String ->\n\
user.display_name().\n\
\n\
pub main(): Unit ->\n\
std.io.Console.println(show(User(\"Ada\"))).\n",
    )
    .expect("failed to write receiver-method module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("display_name("),
        "receiver method should lower as a receiver-first function:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run receiver-method launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Ada\n");
}

/// Verifies declaration-site trait conformance dispatch executes.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project.
/// - A local trait, a struct declaring `implements Trait[Struct]`, and a
///   receiver method satisfying the required trait method.
///
/// Output:
/// - Test passes when `Trait.method(value)` lowers to the matching local
///   receiver-method function and the generated launcher prints the
///   receiver-method result.
///
/// Transformation:
/// - Compiles declaration-site trait dispatch through parse, typecheck,
///   Erlang lowering, `erlc`, and launcher execution, proving the first
///   P0.5e conformance execution slice is not a typechecker-only feature.
#[test]
fn build_command_compiles_declared_implements_trait_dispatch() {
    let dir = make_temp_dir("directory_project_declared_implements_trait_dispatch");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
\n\
pub trait Show[T] {\n\
to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
name: String\n\
}.\n\
\n\
pub constructor User {\n\
(name: String): User -> User(name = name)\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
user.name.\n\
\n\
pub stringify(user: User): String ->\n\
Show.to_string(user).\n\
\n\
pub main(): Unit ->\n\
println(stringify(User(\"Alice\"))).\n",
    )
    .expect("failed to write declaration-site trait dispatch module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("to_string(User)"),
        "trait dispatch should reuse the receiver-method function:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("show:to_string"),
        "local trait dispatch must not emit an unresolved remote trait call:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run declaration-site trait dispatch launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Alice\n");
}

/// Verifies explicit trait impl method dispatch executes.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project.
/// - A local trait, a struct, and a `pub impl Trait[Struct] for Struct`
///   block containing the concrete method body.
///
/// Output:
/// - Test passes when `Trait.method(value)` lowers through the generated
///   typed impl wrapper and the generated launcher prints the impl result.
///
/// Transformation:
/// - Compiles explicit trait impl dispatch through parse, typecheck,
///   Erlang lowering, `erlc`, and launcher execution, proving P0.5e
///   explicit impl bodies are no longer dropped by the backend.
#[test]
fn build_command_compiles_explicit_trait_impl_dispatch() {
    let dir = make_temp_dir("directory_project_explicit_trait_impl_dispatch");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
\n\
pub trait Named[T] {\n\
name(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
name: String\n\
}.\n\
\n\
pub impl Named[ExternalUser] for ExternalUser {\n\
name(value: ExternalUser): String ->\n\
    value.name.\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Named.name(ExternalUser(name = \"Ada\"))).\n",
    )
    .expect("failed to write explicit trait impl dispatch module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("typer_trait_named_name_externaluser_dict"),
        "explicit impl dispatch should emit a typed wrapper:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("named:name"),
        "explicit impl dispatch must not emit an unresolved remote trait call:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run explicit trait impl dispatch launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Ada\n");
}

/// Verifies imported explicit trait impl dispatch executes.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project with a provider module exporting
///   a trait, type, constructor, and public explicit impl.
/// - A consumer entrypoint importing the provider trait and type, then
///   calling `Trait.method(value)`.
///
/// Output:
/// - Test passes when imported trait dispatch resolves through provider
///   interface conformance metadata and the generated launcher prints the
///   provider impl result.
///
/// Transformation:
/// - Compiles a two-module project through interface-cache resolution,
///   typechecking, Erlang lowering, `erlc`, and launcher execution, proving
///   P0.5e.3b has an executable non-aliased imported trait path.
#[test]
fn build_command_compiles_imported_explicit_trait_impl_dispatch() {
    let dir = make_temp_dir("directory_project_imported_explicit_trait_impl_dispatch");
    let project_dir = dir.join("app");
    let dep_dir = dir.join("people");
    let app_dir = project_dir.join("src/app");
    let dep_src = dep_dir.join("src/people");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::create_dir_all(&dep_src).expect("failed to create dependency src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[dependencies]\npeople = { path = \"../people\" }\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        dep_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"people\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write dependency manifest fixture");
    fs::write(
        dep_src.join("Provider.terl"),
        "\
module people.Provider.\n\
\n\
pub trait Named[T] {\n\
name(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
name: String\n\
}.\n\
\n\
pub constructor ExternalUser {\n\
(name: String): ExternalUser -> ExternalUser(name = name)\n\
}.\n\
\n\
pub impl Named[ExternalUser] for ExternalUser {\n\
name(value: ExternalUser): String ->\n\
    value.name.\n\
}.\n\
\n\
pub new_user(name: String): ExternalUser ->\n\
ExternalUser(name).\n",
    )
    .expect("failed to write imported trait provider module");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import people.Provider.{ExternalUser, Named, new_user}.\n\
\n\
render(user: ExternalUser): String ->\n\
Named.name(user).\n\
\n\
pub main(): Unit ->\n\
println(render(new_user(\"Grace\"))).\n",
    )
    .expect("failed to write imported trait consumer module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let main_erl = fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read app_main.erl");
    assert!(
        main_erl.contains("people_provider:typer_trait_named_name_externaluser_dict"),
        "imported trait dispatch should call provider typed wrapper:\n{}",
        main_erl
    );
    let provider_erl = fs::read_to_string(out_dir.join("src/people_provider.erl"))
        .expect("read people_provider.erl");
    assert!(
        provider_erl.contains("typer_trait_named_name_externaluser_dict/2"),
        "public provider impl wrapper should be exported:\n{}",
        provider_erl
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run imported explicit trait impl dispatch launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Grace\n");
}

/// Verifies executable generic-bound trait method dispatch.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project declaring a trait, a concrete
///   explicit impl, and a generic function with a trait bound.
/// - An entrypoint that calls the generic function with concrete `Int`
///   arguments.
///
/// Output:
/// - Test passes when the build succeeds, generated Erlang uses a hidden
///   trait dictionary for the generic function, and the launcher prints the
///   concrete impl result.
///
/// Transformation:
/// - Compiles `same[A](...)[Eq[A]]` through typechecking, hidden dictionary
///   ABI lowering, Erlang compilation, and launcher execution, proving
///   P0.5e.4 is executable for the selected local-bound shape.
#[test]
fn build_command_compiles_generic_bound_trait_dispatch() {
    let dir = make_temp_dir("directory_project_generic_bound_trait_dispatch");
    let project_dir = dir.join("app");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Bool.\n\
\n\
pub trait Eq[A] {\n\
equal(left: A, right: A): Bool.\n\
}.\n\
\n\
pub impl Eq[Int] for Int {\n\
equal(left: Int, right: Int): Bool ->\n\
    left == right.\n\
}.\n\
\n\
same[A](left: A, right: A)[Eq[A]]: Bool ->\n\
Eq.equal(left, right).\n\
\n\
pub main(): Unit ->\n\
println(Bool.to_string(same(2, 2))).\n",
    )
    .expect("failed to write generic-bound trait dispatch module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let main_erl = fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read app_main.erl");
    assert!(
        main_erl.contains("same(_TyperTraitDicteq_a, Left, Right)"),
        "generic function should receive hidden trait dictionary:\n{}",
        main_erl
    );
    assert!(
        main_erl.contains("apply(?MODULE, maps:get('equal', _TyperTraitDicteq_a)"),
        "generic bound call should dispatch through hidden dictionary:\n{}",
        main_erl
    );
    assert!(
        main_erl.contains("#{'equal' => typer_trait_eq_equal_int_dict}"),
        "concrete call should synthesize trait dictionary from typed impl:\n{}",
        main_erl
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generic-bound trait dispatch launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "true\n");
}

/// Verifies direct function-value invocation executes through builds.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project.
/// - A source module passing a named function to a higher-order function
///   that invokes the value with `f.(value)`.
///
/// Output:
/// - Test passes when the generated launcher prints the transformed value.
///
/// Transformation:
/// - Compiles named-function-as-value capture and dedicated
///   function-value invocation through parse, typecheck, Erlang lowering,
///   `erlc`, and launcher execution. This proves `Name(...)` remains a
///   named call while `Expr.(...)` is executable callable-value syntax.
#[test]
fn build_command_compiles_direct_function_value_invocation() {
    let dir = make_temp_dir("directory_project_direct_function_value_invocation");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub increment(value: Int): Int ->\n\
value + 1.\n\
\n\
pub run_callback(value: Int, f: (Int) -> Int): Int ->\n\
f.(value).\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run_callback(41, increment))).\n",
    )
    .expect("failed to write direct function-value invocation module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("run_callback(Value, F)"),
        "higher-order function should keep callable parameter:\n{}",
        erl_source
    );
    assert!(
        erl_source.contains("(F)(Value)"),
        "function-value invocation should lower as a callable value:\n{}",
        erl_source
    );
    assert!(
        erl_source.contains("fun increment/1"),
        "named function should lower as a local function value:\n{}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run direct function-value invocation launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

/// Verifies imported module-member functions can be passed as values.
///
/// Inputs:
/// - A manifest-backed project with `app.Users.index/1`.
/// - A caller importing `app.Users` and passing `Users.index` to a
///   higher-order local function.
///
/// Output:
/// - Test passes when the emitted Erlang captures the imported function as
///   `fun app_users:index/1` and the launcher prints the callback result.
///
/// Transformation:
/// - Builds through parse, import-interface loading, typecheck, syntax
///   lowering, `erlc`, and launcher execution to prove uppercase module-member
///   access can resolve as a typed function value without adding route-specific
///   syntax.
#[test]
fn build_command_compiles_imported_module_member_function_value() {
    let dir = make_temp_dir("directory_project_imported_module_member_function_value");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Users.terl"),
        "\
module app.Users.\n\
\n\
pub index(value: Int): Int ->\n\
value + 1.\n",
    )
    .expect("failed to write provider module");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import app.Users.\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub run_callback(value: Int, f: (Int) -> Int): Int ->\n\
f.(value).\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run_callback(41, Users.index))).\n",
    )
    .expect("failed to write caller module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("fun app_users:index/1"),
        "imported module-member function value should lower as remote fun:\n{}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run imported module-member function-value launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

/// Verifies manifest builds lower receiver-method pipe syntax.
///
/// Inputs:
/// - A manifest-backed `beam-thin` project.
/// - A package-rooted module with a receiver method and a function body
///   using `receiver |> method()`.
///
/// Output:
/// - Test passes when the build emits runnable BEAM-backed artifacts and
///   the generated launcher prints the receiver-method result.
///
/// Transformation:
/// - Compiles receiver-method pipe syntax through parse, typecheck, Erlang
///   lowering, `erlc`, and launcher execution, proving the receiver-pipe
///   typecheck rule has an executable immutable-method backend path.
#[test]
fn build_command_compiles_project_receiver_method_pipe_entrypoint() {
    let dir = make_temp_dir("directory_project_receiver_method_pipe_entrypoint");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
pub struct User {\n\
name: String\n\
}.\n\
\n\
pub constructor User {\n\
(name: String): User -> User(name = name)\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
user.name.\n\
\n\
show(user: User): String ->\n\
user |> display_name().\n\
\n\
pub main(): Unit ->\n\
std.io.Console.println(show(User(\"Ada\"))).\n",
    )
    .expect("failed to write receiver-method pipe module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("display_name("),
        "receiver pipe should lower through receiver-first function:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run receiver-method pipe launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Ada\n");
}

/// Verifies command-style mutable receiver methods execute through builds.
///
/// Inputs:
/// - A manifest-backed project declaring a command-style mutable receiver
///   method with the contextual `mut` receiver marker.
/// - A function sequence that calls the mutable receiver method and then an
///   immutable receiver method on the same source binding.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM-backed artifacts
///   and the launcher prints the updated receiver state.
///
/// Transformation:
/// - Runs the P0.2c command-style mutable receiver ABI through parse,
///   typecheck, CoreIR bridge validation, Erlang lowering, `erlc`, and
///   launcher execution.
#[test]
fn build_command_compiles_command_style_mutable_receiver_method_sequence() {
    let dir = make_temp_dir("directory_project_mutable_receiver_sequence");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
pub struct Cell {\n\
value: String\n\
}.\n\
\n\
pub constructor Cell {\n\
(value: String): Cell -> Cell(value = value)\n\
}.\n\
\n\
pub (mut cell: Cell) replace(value: String): Unit ->\n\
Cell(value).\n\
\n\
pub (cell: Cell) get(): String ->\n\
cell.value.\n\
\n\
run(cell: Cell): String ->\n\
cell.replace(\"new\");\n\
cell.get().\n\
\n\
pub main(): Unit ->\n\
std.io.Console.println(run(Cell(\"old\"))).\n",
    )
    .expect("failed to write mutable receiver module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = replace("),
        "mutable receiver sequence should bind updated receiver:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run mutable receiver launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "new\n");
}

/// Verifies bracket reads execute through `IndexGet` trait wrappers.
///
/// Inputs:
/// - A manifest-backed project declaring an `IndexGet[C, I, T]` trait.
/// - A local `IndexedBox` struct with an explicit `IndexGet` implementation.
/// - A function body using `value[index]` bracket syntax.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints the value returned by the typed trait implementation.
///
/// Transformation:
/// - Runs the N0.2 indexed-read path through parse, typecheck, CoreIR
///   bridge validation, syntax backend trait-wrapper lowering, `erlc`, and
///   launcher execution. The generated Erlang assertion proves bracket
///   syntax does not lower to raw tuple/list indexing for this trait-backed
///   collection shape.
#[test]
fn build_command_compiles_index_read_through_index_get_trait_wrapper() {
    let dir = make_temp_dir("directory_project_index_get_trait_wrapper");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub trait IndexGet[C, I, T] {\n\
get_at(collection: C, index: I): T.\n\
}.\n\
\n\
pub struct IndexedBox {\n\
value: Int\n\
}.\n\
\n\
pub constructor IndexedBox {\n\
(value: Int): IndexedBox -> IndexedBox(value = value)\n\
}.\n\
\n\
pub impl IndexGet[IndexedBox, Int, Int] for IndexedBox {\n\
get_at(collection: IndexedBox, index: Int): Int ->\n\
    collection.value + index.\n\
}.\n\
\n\
read(value: IndexedBox): Int ->\n\
value[1].\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(read(IndexedBox(41)))).\n",
    )
    .expect("failed to write index get module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("typer_trait_indexget_get_at_indexedbox_dict"),
        "indexed read should lower through typed trait wrapper:\n{}",
        erl_text
    );
    assert!(
        !erl_text.contains("element((1) + 1"),
        "indexed read should not lower to raw Erlang tuple indexing:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run index get launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

/// Verifies bracket assignment executes through mutable receiver rebinding.
///
/// Inputs:
/// - A manifest-backed project declaring an `IndexSet[C, I, T]` trait.
/// - A local `IndexedBox` struct implementing `IndexSet` through a mutable
///   receiver `set_at` method.
/// - A function body using `value[index] = next` before reading `value`.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints the value from the rebound receiver.
///
/// Transformation:
/// - Runs the N0.3 indexed-assignment path through parse, typecheck,
///   CoreIR bridge validation, syntax backend mutable receiver lowering,
///   `erlc`, and launcher execution.
#[test]
fn build_command_compiles_index_assignment_through_mutable_receiver_rebinding() {
    let dir = make_temp_dir("directory_project_index_set_receiver_rebinding");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub struct IndexedBox implements IndexSet[IndexedBox, Int, Int] {\n\
value: Int\n\
}.\n\
\n\
pub constructor IndexedBox {\n\
(value: Int): IndexedBox -> IndexedBox(value = value)\n\
}.\n\
\n\
pub (mut box: IndexedBox) set_at(index: Int, value: Int): Unit ->\n\
IndexedBox(value + index).\n\
\n\
pub (box: IndexedBox) current(): Int ->\n\
box.value.\n\
\n\
read(value: IndexedBox): Int ->\n\
value[1] = 41;\n\
value.current().\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(read(IndexedBox(0)))).\n",
    )
    .expect("failed to write index set module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = set_at("),
        "indexed assignment should bind updated receiver:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run index set launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}
