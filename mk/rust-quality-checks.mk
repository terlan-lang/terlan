# ---- Rust implementation quality checks ----

# Checks the pinned cargo-audit version and runs a deny-on-warnings security audit.
rust-security-audit-check:
	@cargo-audit --version | grep -Fx 'cargo-audit 0.22.2'
	cargo-audit audit --deny warnings


# Validates which Rust build features ship in release artifacts.
rust-build-feature-shipping-check: | terlan-tvm-platform-matrix-bootstrap
	$(RUST_TEST) -p terlan --lib --features quality-tools rust_build_feature_shipping_test
	$(TERLAN_QUALITY) rust-build-feature-shipping


# Aliases the build-owner bootstrap for cache-admission consumers
rust-build-cache-bootstrap: terlan-build-owner-bootstrap


# Admits the build under the minimum-free-space policy via terlan-build-cache
rust-build-resource-admission: rust-build-cache-bootstrap
	mkdir -p target/quality
	$(TERLAN_PREPARATION_OWNER) timeout 120s \
		target/debug/terlan-build-cache admit --minimum-free-bytes "$(TERLAN_BUILD_MINIMUM_FREE_BYTES)"


# Audits the Rust incremental build cache
rust-incremental-cache-audit:
	test -x target/debug/terlan-build-cache
	timeout 120s target/debug/terlan-build-cache audit


# Prunes the Rust incremental build cache
rust-incremental-cache-prune:
	test -x target/debug/terlan-build-cache
	timeout 120s target/debug/terlan-build-cache prune


