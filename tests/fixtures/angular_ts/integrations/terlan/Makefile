TERLC ?= terlc
NODE ?= node
PLAYWRIGHT ?= ../../node_modules/.bin/playwright
BUILD_DIR ?= build
APP_BROWSER_TARGET ?= browser-test

.PHONY: all build generate generate-check artifact-check test harness-check browser-test app-ownership-check check run clean

all: check

build:
	$(TERLC) build src --target js.shared --out-dir $(BUILD_DIR)

generate:
	$(NODE) tool/generate_terlan_todo.mjs

generate-check:
	$(NODE) tool/generate_terlan_todo.mjs --check

artifact-check: build
	test -f $(BUILD_DIR)/js/manifest.json
	test -f $(BUILD_DIR)/js/modules/terlan/angular/Todo.js

namespace-manifest:
	TERLAN_BINDGEN_VERSION="$$( $(TERLC) --version | tail -n 1 | awk '{print $$2}' )" $(NODE) tool/generate_ng_namespace_manifest.mjs

namespace-check: namespace-manifest
	rm -rf $(BUILD_DIR)/ng-namespace
	cd ../.. && $(TERLC) bind js-dom --manifest integrations/terlan/generated/ng_namespace_inputs.json --out integrations/terlan/$(BUILD_DIR)/ng-namespace
	$(NODE) tool/check_ng_namespace_bindings.mjs

test: build
	$(NODE) tests/todo_boundary.test.mjs

harness-check: build
	$(NODE) tests/todo_harness_contract.test.mjs

browser-test: build
	$(PLAYWRIGHT) test --config playwright.config.ts

app-ownership-check: generate-check build $(APP_BROWSER_TARGET)
	$(NODE) tests/todo_boundary.test.mjs
	$(NODE) tests/todo_harness_contract.test.mjs

check: generate generate-check artifact-check app-ownership-check namespace-check

run: test

clean:
	rm -rf $(BUILD_DIR)
