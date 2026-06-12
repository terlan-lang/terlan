.PHONY: check test build release-artifact-linux validate-ebnf clean

check:
	cargo check --locked --workspace
	python3 tools/validate_ebnf.py --strict

test:
	cargo test --locked --workspace

build:
	cargo build --locked --bin terlc

validate-ebnf:
	python3 tools/validate_ebnf.py --strict

release-artifact-linux:
	cargo build --release --locked --bin terlc
	mkdir -p dist
	cp target/release/terlc dist/terlc
	tar -C dist -czf dist/terlc-linux-x86_64.tar.gz terlc

clean:
	cargo clean
	rm -rf dist
