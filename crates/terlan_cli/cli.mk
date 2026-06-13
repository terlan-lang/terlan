.PHONY: cli-check cli-build cli-test cli-release-artifact-linux cli-clean

cli-check:
	cargo check --locked --workspace

cli-build:
	cargo build --locked --bin terlc

cli-test:
	cargo test --locked --workspace

cli-release-artifact-linux:
	cargo build --release --locked --bin terlc
	mkdir -p dist
	cp target/release/terlc dist/terlc
	tar -C dist -czf dist/terlc-linux-x86_64.tar.gz terlc

cli-clean:
	cargo clean
	rm -rf dist
