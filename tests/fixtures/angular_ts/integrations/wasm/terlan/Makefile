TERLC ?= terlc
NODE ?= node
BUILD_DIR ?= build

.PHONY: all generate generate-check manifest-check namespace-manifest namespace-check reserved-build-check check clean

all: check

generate:
	$(NODE) tool/generate_terlan_wasm_app.mjs

generate-check:
	$(NODE) tool/generate_terlan_wasm_app.mjs --check

manifest-check:
	$(NODE) tests/wasm_app_contract.test.mjs

namespace-manifest:
	TERLAN_BINDGEN_VERSION="$$( $(TERLC) --version | tail -n 1 | awk '{print $$2}' )" $(NODE) tool/generate_ng_namespace_manifest.mjs

namespace-check: namespace-manifest
	rm -rf $(BUILD_DIR)/ng-namespace
	cd ../../.. && $(TERLC) bind js-dom --manifest integrations/wasm/terlan/generated/ng_namespace_inputs.json --out integrations/wasm/terlan/$(BUILD_DIR)/ng-namespace
	$(NODE) tool/check_ng_namespace_parity.mjs

reserved-build-check:
	mkdir -p $(BUILD_DIR)
	@if $(TERLC) build . --target wasm.browser > $(BUILD_DIR)/reserved-build.out 2>&1; then \
		echo "expected wasm.browser build to remain reserved"; \
		cat $(BUILD_DIR)/reserved-build.out; \
		exit 1; \
	fi
	grep -q "reserved for the Wasm target family" $(BUILD_DIR)/reserved-build.out

check: generate generate-check manifest-check namespace-check reserved-build-check

clean:
	rm -rf $(BUILD_DIR)
