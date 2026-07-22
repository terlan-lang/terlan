
fn validate_native_metadata(package_root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let cargo = match read_toml::<CargoManifest>(&package_root.join("native/Cargo.toml")) {
        Ok(manifest) => manifest,
        Err(message) => {
            diagnostics.push(message);
            return diagnostics;
        }
    };
    expect_eq(
        "native/Cargo.toml",
        "package.name",
        Some(cargo.package.name.as_str()),
        Some("terlan_polars_native"),
        &mut diagnostics,
    );
    if !cargo.features.contains_key("real-polars") {
        diagnostics.push("native/Cargo.toml: features.real-polars is required".to_string());
    }
    match cargo.dependencies.get("polars") {
        Some(CargoDependency::Table { optional }) if optional.unwrap_or(false) => {}
        Some(_) => diagnostics.push(
            "native/Cargo.toml: polars dependency must be an optional table dependency".to_string(),
        ),
        None => diagnostics.push("native/Cargo.toml: missing polars dependency".to_string()),
    }
    if !cargo
        .bin
        .iter()
        .any(|bin| bin.name == "terlan-polars-native-boundary")
    {
        diagnostics
            .push("native/Cargo.toml: missing terlan-polars-native-boundary bin".to_string());
    }
    match cargo.package.metadata.and_then(|metadata| metadata.terlan) {
        Some(metadata) => {
            expect_eq(
                "native/Cargo.toml",
                "package.metadata.terlan.package",
                Some(metadata.package.as_str()),
                Some("terlan-polars"),
                &mut diagnostics,
            );
            expect_eq(
                "native/Cargo.toml",
                "package.metadata.terlan.namespace",
                Some(metadata.namespace.as_str()),
                Some("polars"),
                &mut diagnostics,
            );
        }
        None => diagnostics.push("native/Cargo.toml: missing package.metadata.terlan".to_string()),
    }
    diagnostics
}

fn validate_terlan_surface(package_root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let dataframe = package_root.join("src/polars/DataFrame.terl");
    let source = match read_text(&dataframe) {
        Ok(source) => source,
        Err(message) => {
            diagnostics.push(message);
            return diagnostics;
        }
    };
    for fragment in [
        "module polars.DataFrame.",
        "pub opaque type DataFrame.",
        "pub opaque type LazyFrame.",
        "pub opaque type Series.",
        "pub type DataType =",
        "pub opaque type Expr = String.",
        "pub read_csv(_path: String): Result[DataFrame, Error] ->",
        "pub from_rows(_columns: List[String], _rows: List[List[String]]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) height(): Int ->",
        "pub (_df: DataFrame) width(): Int ->",
        "pub (_df: DataFrame) columns(): List[String] ->",
        "pub (_df: DataFrame) rows(_limit: Int): Result[List[List[String]], Error] ->",
        "pub struct ColumnSchema {",
        "pub (_df: DataFrame) schema(): List[ColumnSchema] ->",
        "pub type Scalar = String | Int | Float | Bool.",
        "pub (_df: DataFrame) filter_eq(_column: String, _value: Scalar): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) sort_by(_column: String, _descending: Bool): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) group_count(_keys: List[String]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) lazy(): LazyFrame ->",
        "pub (_plan: LazyFrame) where_eq(_column: String, _value: Scalar): LazyFrame ->",
        "pub (_plan: LazyFrame) project(_columns: List[String]): LazyFrame ->",
        "pub (_plan: LazyFrame) collect(): Result[DataFrame, Error] ->",
        "pub (_plan: LazyFrame) release(): Unit ->",
        "pub (_df: DataFrame) select(_columns: List[String]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) head(_limit: Int): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) dispose(): Unit ->",
        "pub (_df: DataFrame) select_exprs(_expressions: List[Expr]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) with_columns(_expressions: List[Expr]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) filter(_predicate: Expr): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) group_agg(_keys: List[Expr], _aggregations: List[Expr]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) left_join(_right: DataFrame, _keys: List[String]): Result[DataFrame, Error] ->",
        "pub (_df: DataFrame) concat_vertical(_other: DataFrame): Result[DataFrame, Error] ->",
    ] {
        if !source.contains(fragment) {
            diagnostics.push(format!("{}: missing `{fragment}`", dataframe.display()));
        }
    }
    for (_, operation) in REQUIRED_FUNCTIONS {
        if !source.contains(&format!("@compiler.native {{{operation}}}")) {
            diagnostics.push(format!(
                "{}: missing native operation `{operation}`",
                dataframe.display()
            ));
        }
    }
    for test_name in REQUIRED_TESTS {
        if !package_root.join("test").join(test_name).exists() {
            diagnostics.push(format!("missing package test `{test_name}`"));
        }
    }
    for required in [
        "test/fixtures/people.csv",
        "test/fixtures/groups.csv",
        "test/fixtures/malformed.csv",
        "test/fixtures/nullable.csv",
        "examples/consumer_project/terlan.toml",
        "examples/loaded_helper_project/terlan.toml",
        "examples/polars_getting_started/terlan.toml",
        "examples/polars_expressions/terlan.toml",
        "examples/polars_series/terlan.toml",
        "examples/polars_io_formats/terlan.toml",
        "examples/polars_lazy_full/terlan.toml",
        "examples/polars_relational/terlan.toml",
        "examples/polars_reshape/terlan.toml",
        "examples/polars_advanced_relational/terlan.toml",
        "examples/polars_expression_namespaces/terlan.toml",
    ] {
        if !package_root.join(required).exists() {
            diagnostics.push(format!("missing package fixture `{required}`"));
        }
    }
    diagnostics
}

fn validate_generated_boundary(package_root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let manifest_path = package_root.join("native/generated/polars.DataFrame.native_boundary.json");
    let manifest = match read_text(&manifest_path) {
        Ok(source) => source,
        Err(message) => {
            diagnostics.push(message);
            return diagnostics;
        }
    };
    for (name, operation) in REQUIRED_FUNCTIONS {
        if !manifest.contains(&format!("\"name\": \"{name}\"")) {
            diagnostics.push(format!(
                "{}: missing function `{name}`",
                manifest_path.display()
            ));
        }
        if !manifest.contains(&format!("\"operation\": \"{operation}\"")) {
            diagnostics.push(format!(
                "{}: missing operation `{operation}`",
                manifest_path.display()
            ));
        }
    }

    let rust_stub =
        package_root.join("native/generated/polars_data_frame_native_boundary.native_boundary.rs");
    let rust_source = match read_text(&rust_stub) {
        Ok(source) => source,
        Err(message) => {
            diagnostics.push(message);
            return diagnostics;
        }
    };
    if !rust_source.contains("pub const SOURCE_MODULE: &str = \"polars.DataFrame\";") {
        diagnostics.push(format!(
            "{}: missing source module constant",
            rust_stub.display()
        ));
    }
    for (_, operation) in REQUIRED_FUNCTIONS {
        if !rust_source.contains(operation) {
            diagnostics.push(format!(
                "{}: missing generated operation `{operation}`",
                rust_stub.display()
            ));
        }
    }
    diagnostics
}

fn validate_no_std_native_polars_namespace(package_root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if let Err(message) = scan_text_files(package_root, &mut |path, text| {
        if text.contains("std.native.polars") {
            diagnostics.push(format!(
                "{}: public package namespace must be polars, not std.native.polars",
                path.display()
            ));
        }
    }) {
        diagnostics.push(message);
    }
    diagnostics
}

fn scan_text_files(root: &Path, on_file: &mut dyn FnMut(&Path, &str)) -> Result<(), String> {
    for entry in fs::read_dir(root)
        .map_err(|err| format!("{}: failed to read directory: {err}", root.display()))?
    {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", root.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name == ".git" || name == "target" {
            continue;
        }
        if path.is_dir() {
            scan_text_files(&path, on_file)?;
        } else if path.is_file() {
            match fs::read_to_string(&path) {
                Ok(text) => on_file(&path, &text),
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {}
                Err(err) => return Err(format!("{}: failed to read text: {err}", path.display())),
            }
        }
    }
    Ok(())
}

fn run_native_adapter_tests(package_root: &Path) -> Result<(), String> {
    let manifest = package_root.join("native/Cargo.toml");
    for features in [None, Some("real-polars")] {
        let mut command = Command::new("cargo");
        command
            .args(["test", "--manifest-path"])
            .arg(&manifest)
            .arg("--quiet");
        if let Some(features) = features {
            command.args(["--features", features]);
        }
        let output = command
            .output()
            .map_err(|err| format!("{}: failed to run cargo test: {err}", manifest.display()))?;
        if !output.status.success() {
            return Err(format!(
                "{}: native adapter cargo tests ({}) failed with status {}\nstdout:\n{}\nstderr:\n{}",
                manifest.display(),
                features.unwrap_or("default features"),
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(())
}

fn read_toml<T>(path: &Path) -> QualityResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let text = read_text(path)?;
    basic_toml::from_str(&text).map_err(|err| format!("{}: invalid TOML: {err}", path.display()))
}

fn read_text(path: &Path) -> QualityResult<String> {
    fs::read_to_string(path).map_err(|err| format!("{}: failed to read: {err}", path.display()))
}

fn expect_eq(
    label: &str,
    field: &str,
    actual: Option<&str>,
    expected: Option<&str>,
    diagnostics: &mut Vec<String>,
) {
    if actual != expected {
        diagnostics.push(format!(
            "{label}: {field} expected {:?}, got {:?}",
            expected.unwrap_or("<missing>"),
            actual.unwrap_or("<missing>")
        ));
    }
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[terlan-polars-package] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[derive(Debug, Deserialize)]
struct TerlanManifest {
    package: PackageIdentity,
    native: NativeSection,
}

#[derive(Debug, Deserialize)]
struct ContractManifest {
    package: ContractPackage,
    native: ContractNativeSection,
}

#[derive(Debug, Deserialize)]
struct ContractPackage {
    #[serde(flatten)]
    common: PackageIdentity,
    hex: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackageIdentity {
    name: String,
    namespace: String,
    description: Option<String>,
    license: Option<String>,
    repository: Option<String>,
    compiler: Option<String>,
    #[serde(default)]
    links: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NativeSection {
    rust: NativeRustManifest,
}

#[derive(Debug, Deserialize)]
struct ContractNativeSection {
    rust: ContractNativeRust,
}

#[derive(Debug, Deserialize)]
struct ContractNativeRust {
    #[serde(flatten)]
    common: NativeRustManifest,
    dependencies: ContractNativeDependencies,
}

#[derive(Debug, Deserialize)]
struct NativeRustManifest {
    #[serde(rename = "crate")]
    crate_name: String,
    path: String,
    helper: String,
    helper_env: String,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ContractNativeDependencies {
    polars: PolarsDependency,
}

#[derive(Debug, Deserialize)]
struct PolarsDependency {
    cargo: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: CargoPackage,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    dependencies: BTreeMap<String, CargoDependency>,
    #[serde(default)]
    bin: Vec<CargoBin>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    metadata: Option<CargoPackageMetadata>,
}

#[derive(Debug, Deserialize)]
struct CargoPackageMetadata {
    terlan: Option<CargoTerlanMetadata>,
}

#[derive(Debug, Deserialize)]
struct CargoTerlanMetadata {
    package: String,
    namespace: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CargoDependency {
    Table { optional: Option<bool> },
    Version(serde::de::IgnoredAny),
}

#[derive(Debug, Deserialize)]
struct CargoBin {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn package_boundary_accepts_minimal_external_polars_shape() {
        let root = TempPackage::new("terlan_polars_package_ok");
        root.write_minimal_package();

        let diagnostics = validate_package_boundary(root.path());

        assert_eq!(diagnostics, Vec::<String>::new());
    }

    #[test]
    fn package_boundary_rejects_std_native_polars_namespace_leak() {
        let root = TempPackage::new("terlan_polars_package_namespace_leak");
        root.write_minimal_package();
        fs::write(
            root.path().join("README.md"),
            "Do not import std.native.polars.DataFrame.",
        )
        .expect("write namespace leak");

        let diagnostics = validate_package_boundary(root.path());

        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .contains("public package namespace must be polars, not std.native.polars")));
    }

    struct TempPackage {
        path: PathBuf,
    }

    impl TempPackage {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos();
            let path = env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), unique));
            fs::create_dir_all(&path).expect("create temp package");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_minimal_package(&self) {
            fs::create_dir_all(self.path.join("src/polars")).expect("create src");
            fs::create_dir_all(self.path.join("test/fixtures")).expect("create tests");
            fs::create_dir_all(self.path.join("examples/consumer_project"))
                .expect("create example");
            fs::create_dir_all(self.path.join("examples/loaded_helper_project"))
                .expect("create loaded-helper example");
            fs::create_dir_all(self.path.join("native/generated")).expect("create generated");
            fs::write(
                self.path.join("terlan.toml"),
                r#"[package]
name = "terlan-polars"
namespace = "polars"
description = "Polars DataFrame integration for Terlan"
license = "MIT"
repository = "https://github.com/terlan-lang/terlan-polars"
compiler = ">= 0.0.7"
links = ["https://terlan.org", "https://pola.rs"]

[native.rust]
crate = "terlan_polars_native"
path = "native"
helper = "terlan-polars-native-boundary"
helper_env = "TERLAN_NATIVE_BOUNDARY_HELPER_PATH"
features = ["real-polars"]
"#,
            )
            .expect("write terlan.toml");
            fs::write(
                self.path.join("package.contract.toml"),
                r#"[package]
name = "terlan-polars"
namespace = "polars"
hex = "terlan_polars"

[native.rust]
crate = "terlan_polars_native"
path = "native"
helper = "terlan-polars-native-boundary"
helper_env = "TERLAN_NATIVE_BOUNDARY_HELPER_PATH"
features = ["real-polars"]

[native.rust.dependencies]
polars = { cargo = "polars", status = "feature-gated" }
"#,
            )
            .expect("write package.contract.toml");
            fs::write(
                self.path.join("native/Cargo.toml"),
                r#"[package]
name = "terlan_polars_native"

[features]
real-polars = ["dep:polars"]

[dependencies]
polars = { version = "0.54", optional = true }

[[bin]]
name = "terlan-polars-native-boundary"
path = "src/bin/terlan_polars_native_boundary.rs"

[package.metadata.terlan]
package = "terlan-polars"
namespace = "polars"
"#,
            )
            .expect("write Cargo.toml");
            fs::write(
                self.path.join("src/polars/DataFrame.terl"),
                r#"module polars.DataFrame.

pub opaque type DataFrame.

pub opaque type LazyFrame.

@compiler.native {polars.dataframe.read_csv}
pub read_csv(_path: String): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.from_rows}
pub from_rows(_columns: List[String], _rows: List[List[String]]): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.height}
pub (_df: DataFrame) height(): Int -> native.

@compiler.native {polars.dataframe.width}
pub (_df: DataFrame) width(): Int -> native.

@compiler.native {polars.dataframe.columns}
pub (_df: DataFrame) columns(): List[String] -> native.

@compiler.native {polars.dataframe.rows}
pub (_df: DataFrame) rows(_limit: Int): Result[List[List[String]], Error] -> native.

pub struct ColumnSchema {
    name: String,
    data_type: String
}.

@compiler.native {polars.dataframe.schema}
pub (_df: DataFrame) schema(): List[ColumnSchema] -> native.

pub type Scalar = String | Int | Float | Bool.

@compiler.native {polars.dataframe.filter_eq}
pub (_df: DataFrame) filter_eq(_column: String, _value: Scalar): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.sort_by}
pub (_df: DataFrame) sort_by(_column: String, _descending: Bool): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.group_count}
pub (_df: DataFrame) group_count(_keys: List[String]): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.lazy}
pub (_df: DataFrame) lazy(): LazyFrame -> native.

@compiler.native {polars.lazy_frame.filter_eq}
pub (_plan: LazyFrame) where_eq(_column: String, _value: Scalar): LazyFrame -> native.

@compiler.native {polars.lazy_frame.select}
pub (_plan: LazyFrame) project(_columns: List[String]): LazyFrame -> native.

@compiler.native {polars.lazy_frame.collect}
pub (_plan: LazyFrame) collect(): Result[DataFrame, Error] -> native.

@compiler.native {polars.lazy_frame.dispose}
pub (_plan: LazyFrame) release(): Unit -> native.

@compiler.native {polars.dataframe.select}
pub (_df: DataFrame) select(_columns: List[String]): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.head}
pub (_df: DataFrame) head(_limit: Int): Result[DataFrame, Error] -> native.

@compiler.native {polars.dataframe.dispose}
pub (_df: DataFrame) dispose(): Unit -> native.
"#,
            )
            .expect("write DataFrame.terl");
            let dataframe_path = self.path.join("src/polars/DataFrame.terl");
            let mut dataframe_source =
                fs::read_to_string(&dataframe_path).expect("read minimal DataFrame source");
            dataframe_source.push_str(
                r#"

pub opaque type Series.
pub opaque type Expr = String.
pub type DataType = Int64.

pub (_df: DataFrame) select_exprs(_expressions: List[Expr]): Result[DataFrame, Error] -> native.
pub (_df: DataFrame) with_columns(_expressions: List[Expr]): Result[DataFrame, Error] -> native.
pub (_df: DataFrame) filter(_predicate: Expr): Result[DataFrame, Error] -> native.
pub (_df: DataFrame) group_agg(_keys: List[Expr], _aggregations: List[Expr]): Result[DataFrame, Error] -> native.
pub (_df: DataFrame) left_join(_right: DataFrame, _keys: List[String]): Result[DataFrame, Error] -> native.
pub (_df: DataFrame) concat_vertical(_other: DataFrame): Result[DataFrame, Error] -> native.
"#,
            );
            for (_, operation) in REQUIRED_FUNCTIONS {
                dataframe_source.push_str(&format!("\n@compiler.native {{{operation}}}\n"));
            }
            fs::write(&dataframe_path, dataframe_source).expect("extend minimal DataFrame source");
            for test_name in REQUIRED_TESTS {
                fs::write(self.path.join("test").join(test_name), "module test.")
                    .expect("write test");
            }
            fs::write(self.path.join("test/fixtures/people.csv"), "name\nAda\n")
                .expect("write csv");
            fs::write(self.path.join("test/fixtures/groups.csv"), "city\nLondon\n")
                .expect("write grouped csv");
            fs::write(
                self.path.join("test/fixtures/malformed.csv"),
                "name,age\n\"Ada,36\n",
            )
            .expect("write malformed csv");
            fs::write(
                self.path.join("test/fixtures/nullable.csv"),
                "name,score\nAda,10\nGrace,\n",
            )
            .expect("write nullable csv");
            fs::write(
                self.path.join("examples/consumer_project/terlan.toml"),
                "[package]\n",
            )
            .expect("write consumer manifest");
            fs::write(
                self.path.join("examples/loaded_helper_project/terlan.toml"),
                "[package]\n",
            )
            .expect("write loaded-helper manifest");
            for example in [
                "polars_getting_started",
                "polars_expressions",
                "polars_series",
                "polars_io_formats",
                "polars_lazy_full",
                "polars_relational",
                "polars_reshape",
                "polars_advanced_relational",
                "polars_expression_namespaces",
            ] {
                let directory = self.path.join("examples").join(example);
                fs::create_dir_all(&directory).expect("create required example fixture");
                fs::write(directory.join("terlan.toml"), "[package]\n")
                    .expect("write required example manifest");
            }
            let manifest = REQUIRED_FUNCTIONS
                .iter()
                .map(|(name, operation)| {
                    format!("{{\"name\": \"{name}\", \"operation\": \"{operation}\"}}")
                })
                .collect::<Vec<_>>()
                .join(",");
            fs::write(
                self.path
                    .join("native/generated/polars.DataFrame.native_boundary.json"),
                format!("[{manifest}]"),
            )
            .expect("write generated json");
            let operations = REQUIRED_FUNCTIONS
                .iter()
                .map(|(_, operation)| *operation)
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(
                self.path
                    .join("native/generated/polars_data_frame_native_boundary.native_boundary.rs"),
                format!("pub const SOURCE_MODULE: &str = \"polars.DataFrame\";\n{operations}\n"),
            )
            .expect("write generated rust");
        }
    }

    impl Drop for TempPackage {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
