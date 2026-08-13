use super::*;

const SYNTHETIC: &str = r#"
schema = 1
backend = "vector-test"
device_classes = ["synthetic-vector"]
artifact_formats = ["vector-object"]
dtypes = ["f32"]
layouts = ["row-major"]
address_spaces = ["host", "device"]
resource_classes = ["buffer", "stream"]
asynchronous_operations = ["execute", "transfer"]
capabilities = ["accelerator.execute"]

[[targets]]
triple = "x86_64-unknown-linux-gnu"
availability = "experimental"
artifact_formats = ["vector-object"]

[[toolchains]]
name = "vector-compiler"
version = "1.0.0"
artifact_formats = ["vector-object"]
required = false

[[operations]]
id = "buffer.add"
effects = ["allocate", "execute"]
asynchronous = true

[[kernels]]
id = "add-f32"
artifact_format = "vector-object"
artifact = "kernels/add.vo"
symbol = "vector_add_f32"
target_architectures = ["vector-v1"]
max_shared_memory_bytes = 0

[[kernels.parameters]]
name = "output"
dtype = "f32"
address_space = "device"
access = "write"
"#;

#[test]
fn synthetic_backend_descriptor_is_normalized_without_backend_special_cases() {
    let descriptor = AcceleratorDescriptor::parse(SYNTHETIC, Path::new("accelerator.toml"))
        .expect("synthetic descriptor");
    assert_eq!(descriptor.backend, "vector-test");
    assert_eq!(
        descriptor.kernels[0].parameters[0].access,
        AcceleratorAccess::Write
    );
}

#[test]
fn malformed_descriptors_fail_before_native_package_loading() {
    let duplicate = SYNTHETIC.replace("dtypes = [\"f32\"]", "dtypes = [\"f32\", \"f32\"]");
    let duplicate_error = AcceleratorDescriptor::parse(&duplicate, Path::new("accelerator.toml"))
        .expect_err("duplicate dtype");
    assert!(
        duplicate_error
            .contains("accelerator.toml:6:18: accelerator `dtypes` contains duplicate `f32`"),
        "{duplicate_error}"
    );

    let escaped = SYNTHETIC.replace("kernels/add.vo", "../kernels/add.vo");
    assert!(
        AcceleratorDescriptor::parse(&escaped, Path::new("accelerator.toml"))
            .expect_err("escaped artifact")
            .contains("package-relative without traversal")
    );

    let unknown = SYNTHETIC.replace("schema = 1", "schema = 9");
    assert!(
        AcceleratorDescriptor::parse(&unknown, Path::new("accelerator.toml"))
            .expect_err("unknown schema")
            .contains("unsupported accelerator descriptor schema `9`")
    );

    let dtype = SYNTHETIC.replace("dtypes = [\"f32\"]", "dtypes = [\"opaque-word\"]");
    let error = AcceleratorDescriptor::parse(&dtype, Path::new("accelerator.toml"))
        .expect_err("unsupported dtype");
    assert!(error.contains("accelerator.toml:6:11"), "{error}");
    assert!(error.contains("unsupported scalar type `opaque-word`"));
}

#[test]
fn accelerator_package_closure_resolves_unique_capability_owners() {
    let provider = package("provider", &["tensor.storage"], &[]);
    let consumer = package("consumer", &["tensor.compute"], &["tensor.storage"]);
    let closure =
        AcceleratorDependencyClosure::resolve(vec![consumer, provider], Path::new("terlan.toml"))
            .expect("valid closure");

    assert_eq!(closure.packages[0].package, "consumer");
    assert_eq!(closure.capability_owners["tensor.storage"], "provider");
}

#[test]
fn accelerator_package_closure_rejects_ambiguous_or_missing_owners() {
    let duplicate = AcceleratorDependencyClosure::resolve(
        vec![
            package("provider-a", &["tensor.storage"], &[]),
            package("provider-b", &["tensor.storage"], &[]),
        ],
        Path::new("terlan.toml"),
    )
    .expect_err("duplicate owner");
    assert!(duplicate.contains("capability `tensor.storage` has duplicate owners"));

    let missing = AcceleratorDependencyClosure::resolve(
        vec![package("consumer", &[], &["tensor.storage"])],
        Path::new("terlan.toml"),
    )
    .expect_err("missing owner");
    assert!(missing.contains("requires unowned capability `tensor.storage`"));
    assert!(
        missing.contains("consumer/accelerator.toml:12:17"),
        "{missing}"
    );
}

#[test]
fn accelerator_package_closure_rejects_cycles_and_incompatible_targets() {
    let cycle = AcceleratorDependencyClosure::resolve(
        vec![
            package("provider-a", &["capability.a"], &["capability.b"]),
            package("provider-b", &["capability.b"], &["capability.a"]),
        ],
        Path::new("terlan.toml"),
    )
    .expect_err("capability cycle");
    assert!(cycle.contains("capability dependency cycle"));

    let mut provider = package("provider", &["tensor.storage"], &[]);
    provider.descriptor.targets[0].triple = "aarch64-unknown-linux-gnu".to_string();
    let mismatch = AcceleratorDependencyClosure::resolve(
        vec![provider, package("consumer", &[], &["tensor.storage"])],
        Path::new("terlan.toml"),
    )
    .expect_err("target mismatch");
    assert!(mismatch.contains("no common available target"));
    assert!(mismatch.contains("x86_64-unknown-linux-gnu"));
    assert!(mismatch.contains("aarch64-unknown-linux-gnu"));
}

fn package(
    name: &str,
    capabilities: &[&str],
    requirements: &[&str],
) -> AcceleratorPackageDescriptor {
    let render = |values: &[&str]| {
        values
            .iter()
            .map(|value| format!("\"{value}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let source = SYNTHETIC.replace(
        "capabilities = [\"accelerator.execute\"]",
        &format!(
            "capabilities = [{}]\nrequirements = [{}]",
            render(capabilities),
            render(requirements)
        ),
    );
    let descriptor_path = format!("{name}/accelerator.toml");
    let descriptor = AcceleratorDescriptor::parse(&source, Path::new(&descriptor_path))
        .expect("synthetic descriptor");
    assert_eq!(
        descriptor.capabilities,
        capabilities
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>()
    );
    AcceleratorPackageDescriptor {
        package: name.to_string(),
        version: "1.0.0".to_string(),
        source: descriptor_path,
        descriptor,
    }
}
