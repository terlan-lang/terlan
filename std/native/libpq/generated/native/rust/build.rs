fn main() {
    let mut probe = pkg_config::Config::new();
    probe.atleast_version("14.0");
    let library = probe
        .probe("libpq")
        .expect("pkg-config must resolve external C package libpq");
    let mut c_build = cc::Build::new();
    c_build.file("c/terlan_libpq.c");
    for include_path in &library.include_paths {
        c_build.include(include_path);
    }
    c_build
        .include("include")
        .include(".")
        .warnings_into_errors(true)
        .flag_if_supported("-std=c11")
        .compile("terlan_native_boundary_c_abi");
    println!("cargo:rerun-if-changed=include");
    println!("cargo:rerun-if-changed=c");
}
