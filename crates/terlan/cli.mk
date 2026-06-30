# Terlan CLI compiler-path targets.
#
# This file is included by the root Makefile. Target names remain public from the
# repository root, but CLI-specific recipes live with the CLI crate.

TERLC := $(CARGO) run -p terlan --
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh
TERLC_EXACT_TEST := $(EXACT_CARGO_TEST) -p terlan
BROWSER_PACKAGE_PREFLIGHT_DIR ?= /tmp/terlan-0-0-5-browser-preflight
STATIC_PROFILE_PREFLIGHT_DIR ?= /tmp/terlan_static_preflight
STATIC_DOCS_PREFLIGHT_DIR ?= /tmp/terlan_static_docs_preflight
WEB_PROFILE_PREFLIGHT_DIR ?= /tmp/terlan_web_profile_preflight

.PHONY: cli-help cli-check cli-build cli-test cli-test-fast cli-test-full cli-test-release cli-release-artifact-current cli-release-artifact-linux cli-clean vm-artifact-check cli-terlan-vm-compiler-bridge-check typecheck-fixture emit-fixture smoke browser-package-preflight js-stdlib-smoke-check static-profile-preflight static-docs-check web-profile-preflight serve-static-smoke serve-web-smoke static-command-check http-router-check http-observability-check http-tls-check web-compose-check template-contract-check private-field-check db-command-check repl-check sql-form-check sql-runtime-check api-schema-check runtime-release-dependency-check release-0-0-4-preflight release-0-0-5-preflight formal-cli-phase-contract-gate formal-cli-build-gate formal-cli-js-gate formal-cli-rust-gate formal-cli-doc-gate formal-cli-a0-50-template-frontend-gate formal-cli-a0-54-constructor-contract-gate formal-cli-a0-55-function-clause-contract-gate formal-cli-a0-56-primary-expression-contract-gate formal-cli-a0-57-keyword-expression-contract-gate formal-cli-a0-58-calls-and-references-contract-gate formal-cli-a0-59-data-form-contract-gate formal-cli-a0-60-pattern-contract-gate formal-cli-a0-61-lexical-and-name-contract-gate formal-cli-a0-62-template-boundary-contract-gate formal-incremental-gate formal-phase-gate formal-directory-phase-gate

cli-help:
	@echo "  make typecheck-fixture - terlan check fixture"
	@echo "  make emit-fixture      - emit fixture .erl/.typi to $(OUT_DIR)"
	@echo "  make smoke             - emit + erlc + runtime smoke test"
	@echo "  make browser-package-preflight - build and validate a JS browser package"
	@echo "  make js-stdlib-smoke-check - run bounded generated std.js test coverage"
	@echo "  make static-profile-preflight - build and validate a static profile site"
	@echo "  make static-docs-check - build and validate a docs-shaped static site"
	@echo "  make web-profile-preflight - scaffold and validate a web profile package"
	@echo "  make serve-static-smoke - run static profile serve smoke"
	@echo "  make serve-web-smoke - run web profile serve smoke"
	@echo "  make static-command-check - run public static command wrapper regressions"
	@echo "  make http-router-check - run HTTP router matcher and route-validation regressions"
	@echo "  make http-observability-check - run HTTP log/error/header regressions"
	@echo "  make http-tls-check - run HTTP TLS manifest and serve guard regressions"
	@echo "  make web-compose-check - run web-profile Docker Compose contract regressions"
	@echo "  make template-contract-check - run typed template metadata/render regressions"
	@echo "  make private-field-check - run private struct field visibility regressions"
	@echo "  make db-command-check - run Postgres migration command regressions"
	@echo "  make repl-check - run REPL evaluator regressions"
	@echo "  make sql-form-check - run typed SQL form parser/typechecker regressions"
	@echo "  make sql-runtime-check - run typed SQL runtime emission regressions"
	@echo "  make api-schema-check - run API contract, OpenAPI emit, and client import regressions"
	@echo "  make runtime-release-dependency-check - require committed live Postgres/TLS runtime dependencies"
	@echo "  make release-0-0-4-preflight - run current 0.0.4 JS target release gate"
	@echo "  make release-0-0-5-preflight - run current 0.0.5 web/editor release gate"
	@echo "  make release-artifact-current - build and smoke-test the current platform artifact"
	@echo "  make vm-artifact-check - build and smoke-test the standalone terlan-vm artifact"
	@echo "  make terlan-vm-compiler-bridge-check - compare OTP and experimental VM source execution"
	@echo "  make formal-cli-phase-contract-gate - run CLI phase-contract golden/parity regressions"
	@echo "  make formal-cli-build-gate - run CLI build artifact/debug-map regressions"
	@echo "  make formal-cli-js-gate - run CLI JavaScript/Oxc output regressions"
	@echo "  make formal-cli-rust-gate - run CLI Rust/native neutrality probe regressions"
	@echo "  make formal-cli-doc-gate - run CLI formal documentation regressions"
	@echo "  make formal-cli-a0-50-template-frontend-gate - run A0.50 normalized template frontend input regression"
	@echo "  make formal-cli-a0-54-constructor-contract-gate - run A0.54 constructor contract regressions"
	@echo "  make formal-cli-a0-55-function-clause-contract-gate - run A0.55 function/clause contract regressions"
	@echo "  make formal-cli-a0-56-primary-expression-contract-gate - run A0.56 primary-expression contract regressions"
	@echo "  make formal-cli-a0-57-keyword-expression-contract-gate - run A0.57 keyword-expression contract regressions"
	@echo "  make formal-cli-a0-58-calls-and-references-contract-gate - run A0.58 calls-and-references contract regressions"
	@echo "  make formal-cli-a0-59-data-form-contract-gate - run A0.59 data-form contract regressions"
	@echo "  make formal-cli-a0-60-pattern-contract-gate - run A0.60 pattern contract regressions"
	@echo "  make formal-cli-a0-61-lexical-and-name-contract-gate - run A0.61 lexical/name contract regressions"
	@echo "  make formal-cli-a0-62-template-boundary-contract-gate - run A0.62 template boundary contract regressions"
	@echo "  make formal-incremental-gate - run CLI incremental dependency-closure regression"
	@echo "  make formal-phase-gate - run formal phase determinism regression gate"
	@echo "  make formal-directory-phase-gate - run deterministic directory-mode phase-manifest gate"

cli-check:
	$(CARGO) check --locked --workspace

cli-build:
	$(CARGO) build --locked --bin terlc --bin terlan-vm

cli-test:
	$(MAKE) --no-print-directory cli-test-fast

cli-test-fast:
	$(CARGO) test --locked --workspace --bins --no-run
	$(MAKE) --no-print-directory vm-artifact-check
	$(TERLC_EXACT_TEST) tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_source_and_beam_for_single_file -- --exact

cli-test-full:
	$(CARGO) build --locked --bin terlc
	PATH="$(CURDIR)/target/debug:$$PATH" $(CARGO) test --locked --workspace

cli-test-release: cli-test-full

cli-release-artifact-current:
	$(CARGO) build --release --locked --bin terlc --bin terlan-vm
	mkdir -p dist
	$(PYTHON) tools/package_release_artifact.py package

cli-release-artifact-linux:
	TERLAN_RELEASE_OS=Linux TERLAN_RELEASE_ARCH=x86_64 $(MAKE) cli-release-artifact-current

cli-clean:
	$(CARGO) clean
	rm -rf dist

vm-artifact-check:
	$(CARGO) build --locked --bin terlan-vm
	rm -rf /tmp/terlan_vm_artifact_check
	mkdir -p /tmp/terlan_vm_artifact_check
	printf '%s\n' 'module vm_artifact.Main.' '' 'import std.io.Console.{println}.' '' 'pub main(): Unit ->' '    println("hello from terlan-vm").' > /tmp/terlan_vm_artifact_check/Main.terl
	test "$$(target/debug/terlan-vm run /tmp/terlan_vm_artifact_check/Main.terl)" = "hello from terlan-vm"

cli-terlan-vm-compiler-bridge-check:
	$(CARGO) build --locked --bin terlc --bin terlan-vm
	rm -rf /tmp/terlan_vm_compiler_bridge_check
	mkdir -p /tmp/terlan_vm_compiler_bridge_check/src/app
	printf '%s\n' '[package]' 'name = "app"' 'version = "0.0.1"' '' '[build]' 'source_roots = ["src"]' 'artifact = "beam-thin"' > /tmp/terlan_vm_compiler_bridge_check/terlan.toml
	printf '%s\n' 'module app.Main.' '' 'import std.io.Console.{println}.' '' 'pub main(): Unit ->' '    println("hello from Terlan VM bridge").' > /tmp/terlan_vm_compiler_bridge_check/src/app/Main.terl
	otp_output="$$(target/debug/terlc --out-dir /tmp/terlan_vm_compiler_bridge_check/_build run /tmp/terlan_vm_compiler_bridge_check)"; \
	if [ "$$otp_output" != "hello from Terlan VM bridge" ]; then \
		printf '%s\n' "terlan-vm compiler bridge check failed in OTP lane"; \
		printf '%s\n' "$$otp_output"; \
		exit 1; \
	fi; \
	vm_output="$$(target/debug/terlan-vm run /tmp/terlan_vm_compiler_bridge_check/src/app/Main.terl)"; \
	if [ "$$vm_output" != "$$otp_output" ]; then \
		printf '%s\n' "terlan-vm compiler bridge check failed in experimental VM lane"; \
		printf '%s\n' "otp: $$otp_output"; \
		printf '%s\n' "vm:  $$vm_output"; \
		exit 1; \
	fi

typecheck-fixture:
	$(TERLC) check $(FIXTURE)

emit-fixture:
	mkdir -p $(OUT_DIR)
	$(TERLC) emit $(FIXTURE) --out-dir $(OUT_DIR)

smoke: emit-fixture
	erlc $(OUT_DIR)/mathx.erl
	erl -noshell -pa $(OUT_DIR) -eval 'io:format("~p~n", [mathx:add(41)]), halt().'

browser-package-preflight:
	rm -rf $(BROWSER_PACKAGE_PREFLIGHT_DIR)
	mkdir -p $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets
	printf '%s\n' 'module app.' '' 'import css "./assets/app.css" as AppCss.' 'import file "./assets/logo.txt" as Logo.' '' 'pub value(): Int ->' '    1.' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/app.terl
	printf '%s\n' 'body { color: black; }' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets/app.css
	printf '%s\n' 'terlan' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets/logo.txt
	$(TERLC) --target-profile js.browser --out-dir $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build build $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src --target js.browser
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/js/manifest.json
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/js/modules/app.js
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/index.html
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/manifest.json
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/assets/js/modules/app.js
	$(PYTHON) -c "import json,pathlib,sys; root=pathlib.Path(sys.argv[1]); manifest=json.loads((root/'manifest.json').read_text()); assert manifest['schema']=='terlan-web-build-v1', manifest; assert manifest['target_profile']=='js.browser', manifest; assert manifest['source_js_manifest']=='../js/manifest.json', manifest; assert (root/manifest['index']).is_file(), manifest['index']; assets=manifest['assets']; assert assets, manifest; missing=[entry['web_relative_path'] for entry in assets if not (root/entry['web_relative_path']).is_file()]; assert not missing, missing; kinds=[entry['kind'] for entry in assets]; assert 'javascript-module' in kinds, kinds; assert 'asset-css' in kinds, kinds; assert 'asset-file' in kinds, kinds" $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web
	$(TERLC) serve $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web --check

js-stdlib-smoke-check:
	$(TERLC) test std/js/StringTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/ArrayTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/MapTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/SetTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/dom/DocumentTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/dom/HTMLElementTest.terl --target js

static-profile-preflight:
	rm -rf $(STATIC_PROFILE_PREFLIGHT_DIR)
	$(TERLC) init $(STATIC_PROFILE_PREFLIGHT_DIR) --profile static
	$(TERLC) static emit $(STATIC_PROFILE_PREFLIGHT_DIR)/src/terlan_static_preflight/Site.terl --out-dir $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web --validate-output --base-path /static-preflight
	test -f $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web/index.html
	grep -F '<base href="/static-preflight/">' $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web/index.html
	$(TERLC) static check $(STATIC_PROFILE_PREFLIGHT_DIR)/src/terlan_static_preflight/Site.terl --out-dir $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web --base-path /static-preflight
	$(TERLC) static serve $(STATIC_PROFILE_PREFLIGHT_DIR)/src/terlan_static_preflight/Site.terl --out-dir $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web --validate-output --base-path /static-preflight --check

static-docs-check:
	rm -rf $(STATIC_DOCS_PREFLIGHT_DIR)
	$(TERLC) init $(STATIC_DOCS_PREFLIGHT_DIR) --profile static
	mkdir -p $(STATIC_DOCS_PREFLIGHT_DIR)/content/guides $(STATIC_DOCS_PREFLIGHT_DIR)/content/api
	printf '%s\n' 'module terlan_static_docs_preflight.Site.' '' 'import css "../../assets/site.css" as SiteCss.' 'import file "../../assets/logo.txt" as Logo.' 'import file "../../assets/site.terl.json" as SiteJson.' 'import file "../../assets/deploy.terl.yaml" as DeployYaml.' 'import file "../../assets/config.terl.toml" as ConfigToml.' 'import markdown "../../content/index.terl.md" as HomeContent.' 'import markdown "../../content/guides/install.terl.md" as InstallContent.' 'import markdown "../../content/api/router.terl.md" as RouterContent.' '' 'template Layout from "../../templates/layout.terl.html" {' '    title: String' '}.' > $(STATIC_DOCS_PREFLIGHT_DIR)/src/terlan_static_docs_preflight/Site.terl
	printf '%s\n' 'main { max-width: 72rem; }' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/site.css
	printf '%s\n' 'terlan docs' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/logo.txt
	printf '%s\n' '{"name": "terlan", "version": $${version}}' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/site.terl.json
	printf '%s\n' 'site:' '  name: $${name}' '  deploy: github-pages' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/deploy.terl.yaml
	printf '%s\n' 'name = $${name}' 'target = "github-pages"' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/config.terl.toml
	printf '%s\n' '@page { title = "Install", layout = "Layout" }' '' '# Install' '' 'Run `terlc init docs --profile static`.' > $(STATIC_DOCS_PREFLIGHT_DIR)/content/guides/install.terl.md
	printf '%s\n' '@page { title = "Router", layout = "Layout" }' '' '# Router' '' 'Static docs can describe typed routes.' > $(STATIC_DOCS_PREFLIGHT_DIR)/content/api/router.terl.md
	$(TERLC) static emit $(STATIC_DOCS_PREFLIGHT_DIR)/src/terlan_static_docs_preflight/Site.terl --out-dir $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web --validate-output --base-path /terlan
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/index.html
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/guides/install/index.html
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/api/router/index.html
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.css
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/logo.txt
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.terl.json
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/deploy.terl.yaml
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/config.terl.toml
	grep -F '<base href="/terlan/">' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/index.html
	grep -F '<base href="/terlan/">' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/guides/install/index.html
	grep -F '<base href="/terlan/">' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/api/router/index.html
	grep -F 'main { max-width: 72rem; }' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.css
	grep -F 'terlan docs' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/logo.txt
	grep -F '"version": $${version}' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.terl.json
	grep -F 'name: $${name}' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/deploy.terl.yaml
	grep -F 'name = $${name}' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/config.terl.toml
	$(TERLC) static check $(STATIC_DOCS_PREFLIGHT_DIR)/src/terlan_static_docs_preflight/Site.terl --out-dir $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web --base-path /terlan

static-command-check:
	$(PYTHON) tools/check_static_route_boundary.py
	$(TERLC_EXACT_TEST) commands::static_site::mod_test::static_check_args_adds_check_and_validation_flags -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::mod_test::static_check_args_preserves_existing_flags -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_infer_nested_content_path -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_infer_index_content_paths -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_infer_generated_relative_content_imports -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_default_title_from_first_heading -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_prefer_explicit_title_over_heading -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_use_page_route_override -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_reject_duplicate_paths -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_reject_parent_directory_segments -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::run_cli_static_emit_accepts_out_dir_after_source_path -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::run_cli_static_check_accepts_out_dir_after_source_path -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::parse_static_routes_text_accepts_compact_singular_route -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::parse_static_routes_text_accepts_compact_route_block -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_injects_base_path_when_requested -- --exact

web-profile-preflight:
	rm -rf $(WEB_PROFILE_PREFLIGHT_DIR)
	$(TERLC) init $(WEB_PROFILE_PREFLIGHT_DIR) --profile web
	$(TERLC) --target-profile js.browser --out-dir $(WEB_PROFILE_PREFLIGHT_DIR)/_build build $(WEB_PROFILE_PREFLIGHT_DIR) --target js.browser
	test -f $(WEB_PROFILE_PREFLIGHT_DIR)/_build/js/manifest.json
	test -f $(WEB_PROFILE_PREFLIGHT_DIR)/_build/web/manifest.json
	$(PYTHON) -c "import json,pathlib,sys; manifest=json.loads(pathlib.Path(sys.argv[1]).read_text()); responses=manifest.get('static_responses', []); handlers=manifest.get('handlers', []); static_routes={(r.get('method'), r.get('route')) for r in responses}; handler_routes={(r.get('method'), r.get('route')) for r in handlers}; required_static={('GET','/users/:id'),('GET','*'),('HEAD','*'),('OPTIONS','*')}; missing_static=required_static-static_routes; assert not missing_static, {'missing_static_routes': sorted(missing_static), 'responses': responses}; assert ('GET','/') in handler_routes, {'handlers': handlers}" $(WEB_PROFILE_PREFLIGHT_DIR)/_build/web/manifest.json
	$(TERLC) serve $(WEB_PROFILE_PREFLIGHT_DIR)/_build/web --check

serve-static-smoke: static-profile-preflight

serve-web-smoke: web-profile-preflight

http-router-check:
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::discover_web_handlers_from_modules_extracts_router_builder_calls -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::discover_web_handlers_from_modules_extracts_receiver_router_builder_calls -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::discover_web_handlers_from_modules_extracts_grouped_router_builder_calls -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::write_browser_package_serializes_discovered_router_handlers -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::discover_web_error_handler_from_modules_extracts_router_error_handler -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::write_browser_package_serializes_router_error_handler -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_prefers_explicit_head -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_route_params -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_typed_route_params -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_rejects_invalid_int_route_param -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_bool_route_params -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_decodes_route_params -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_wildcard_route -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_decodes_wildcard_route_params -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_rejects_invalid_utf8_route_param -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_applies_route_precedence -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_canonical_fallback_route -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_applies_canonical_fallback_precedence -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_accepts_options_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_rejects_non_final_wildcard -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_rejects_empty_route_segment -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_routes_rejects_same_shape_param_routes -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_routes_rejects_colon_and_typed_param_same_shape -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_routes_rejects_duplicate_fallback_shapes -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_accepts_request_plus_route_params -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::validate_handler_rejects_route_param_arity_mismatch -- --exact

http-observability-check:
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_handler_log_line_includes_handler_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_handler_log_line_includes_optional_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_static_log_line_includes_asset_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_static_route_log_line_includes_route_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_file_route_log_line_includes_route_and_file_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_dev_error_page_includes_escaped_handler_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_dev_error_page_omits_absent_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::build_http_response_preserves_server_response_contract -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::build_http_response_appends_validated_dynamic_headers -- --exact

http-tls-check:
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_accepts_absent_server_tls_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_server_tls_auto_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_server_tls_internal_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_server_tls_manual_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_auto_without_domains -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_auto_manual_or_internal_fields -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_internal_with_public_fields -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_manual_acme_provider -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_manual_without_key -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_without_mode -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_accepts_adjacent_project_manifest_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_invalid_adjacent_project_manifest_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_missing_manual_tls_files -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_missing_manual_tls_ca_file -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_manual_tls_paths_outside_project -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::runtime_tls_config_returns_none_for_plain_http_package -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::runtime_tls_config_rejects_invalid_manual_tls_files -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::runtime_tls_config_accepts_manual_certificate_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::runtime_tls_config_accepts_internal_local_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_runtime_plan_defaults_to_lets_encrypt_production -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_runtime_plan_preserves_zerossl_fallback_provider -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_domain_identifiers_preserve_dns_names -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_domain_identifiers_reject_empty_domains -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_contact_strings_wrap_optional_email -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::pending_http01_challenges_select_pending_http_challenges -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::pending_http01_challenges_skip_valid_authorizations -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::pending_http01_challenges_reject_missing_http01 -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::generate_acme_csr_returns_der_and_private_key_pem -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::issue_acme_certificate_cache_rejects_zerossl_before_network -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_account_credentials_round_trip_through_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_account_credentials_cache_reports_invalid_json -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_http01_challenge_cache_writes_valid_token -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_http01_challenge_cache_rejects_invalid_token -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::acme_certificate_cache_write_feeds_runtime_tls_config -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::runtime_tls_config_accepts_auto_tls_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::tls_test::runtime_tls_config_rejects_auto_tls_without_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_returns_404_for_missing_acme_http01_challenge -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_invalid_acme_http01_token -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_keeps_acme_like_static_files_for_plain_http_package -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::run_live_serve_rejects_auto_tls_without_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::serve_web_package_rejects_auto_tls_without_certificate_cache -- --exact

web-compose-check:
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_accepts_postgres_dev_service -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_accepts_long_loopback_postgres_port -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_accepts_list_form_postgres_environment -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_empty_map_form_postgres_environment -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_empty_list_form_postgres_environment -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_malformed_yaml -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_missing_postgres_service -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_disabled_postgres_healthcheck -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_postgres_healthcheck_without_test -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_postgres_healthcheck_none_test -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_postgres_without_healthcheck -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::validate_project_compose_rejects_public_postgres_port_binding -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::start_project_compose_dependencies_ignores_missing_compose -- --exact
	$(TERLC_EXACT_TEST) commands::serve::compose_check::compose_test::docker_compose_up_command_targets_postgres_service_only -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_accepts_adjacent_postgres_compose -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_invalid_adjacent_postgres_compose -- --exact

template-contract-check:
	$(PYTHON) tools/check_html_boundary.py
	$(TERLC_EXACT_TEST) commands::artifacts::artifacts_test::collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::artifacts::artifacts_test::collect_syntax_template_frontend_inputs_rejects_template_metadata_mismatch -- --exact
	$(TERLC_EXACT_TEST) commands::artifacts::artifacts_test::collect_syntax_markdown_frontend_inputs_preserves_page_metadata -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_prop_signature_rejects_duplicate_props -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_prop_signature_rejects_reserved_children_prop -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_slot_typecheck_rejects_record_value_in_text_context -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_slot_typecheck_accepts_scalar_struct_field_in_text_context -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_slot_typecheck_rejects_html_fragment_in_attribute_context -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_slot_typecheck_accepts_arithmetic_expression_in_text_context -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_slot_typecheck_accepts_receiver_method_expression_in_attribute_context -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_component_prop_accepts_expression_slot_matching_expected_type -- --exact
	$(TERLC_EXACT_TEST) validation::template_contract::template_contract_test::template_component_prop_rejects_expression_slot_mismatching_expected_type -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::renders_named_template_call -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::renders_positional_template_call_with_default_prop -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::rejects_template_call_missing_required_prop -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::rejects_template_call_unknown_named_prop -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::rejects_template_call_duplicate_named_prop -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::renders_template_text_slot_escaped_by_default -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::render::render_test::renders_template_attribute_slot_escaped_by_default -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_accepts_template_html_route_return_type -- --exact

private-field-check:
	$(EXACT_CARGO_TEST) -p terlan expression_test::syntax_output_accepts_local_private_struct_field_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan expression_test::syntax_output_accepts_local_private_struct_field_update -- --exact
	$(EXACT_CARGO_TEST) -p terlan expression_test::syntax_output_accepts_local_private_struct_field_pattern -- --exact
	$(EXACT_CARGO_TEST) -p terlan expression_test::syntax_output_rejects_bare_access_to_private_struct_field -- --exact
	$(EXACT_CARGO_TEST) -p terlan expression_test::syntax_output_rejects_bare_update_to_private_struct_field -- --exact
	$(EXACT_CARGO_TEST) -p terlan expression_test::syntax_output_rejects_bare_pattern_for_private_struct_field -- --exact
	$(EXACT_CARGO_TEST) -p terlan import_test::syntax_output_rejects_imported_private_struct_field_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan import_test::syntax_output_rejects_imported_private_struct_field_update -- --exact
	$(EXACT_CARGO_TEST) -p terlan import_test::syntax_output_rejects_imported_private_struct_field_pattern -- --exact

db-command-check:
	$(PYTHON) tools/check_db_command_boundary.py
	$(TERLC_EXACT_TEST) commands::db::mod_test::parse_db_command_accepts_help_for_documented_subcommands -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::parse_db_command_accepts_migrate_database_url_and_directory -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::parse_db_command_accepts_rebuild_with_dev_and_directory -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::parse_db_command_accepts_reset_with_dev_and_directory -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::parse_db_command_accepts_status_database_url_and_directory -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::parse_db_command_rejects_duplicate_dev_flag -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_new_creates_valid_migration_template -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_validate_accepts_valid_migration_directory -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_migrate_validates_then_reports_unreachable_executor -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_status_with_database_url_reports_unreachable_history_loader -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_rebuild_rejects_missing_dev_flag -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_rebuild_with_dev_rejects_non_development_database_url -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::run_reset_with_dev_validates_then_reports_unreachable_executor -- --exact
	$(TERLC_EXACT_TEST) commands::db::mod_test::live_postgres_url_reports_stable_skip_message_when_unconfigured -- --exact
	$(TERLC_EXACT_TEST) commands::db::migration_test::split_migration_sections_accepts_up_and_down -- --exact
	$(TERLC_EXACT_TEST) commands::db::migration_test::split_migration_sections_rejects_missing_up -- --exact
	$(TERLC_EXACT_TEST) commands::db::migration_test::migration_file_inventory_rejects_duplicate_timestamps -- --exact
	$(TERLC_EXACT_TEST) commands::db::migration_test::migration_history_table_sql_defines_required_columns -- --exact
	$(TERLC_EXACT_TEST) commands::db::migration_test::migration_status_classifies_applied_pending_missing_and_divergent -- --exact
	$(TERLC_EXACT_TEST) commands::db::migration_test::pending_migration_engine_inputs_rejects_divergent_history -- --exact
	$(TERLC_EXACT_TEST) commands::db::history::history_test::applied_migration_from_postgres_row_accepts_valid_row -- --exact
	$(TERLC_EXACT_TEST) commands::db::history::history_test::applied_migration_from_postgres_row_rejects_missing_column -- --exact
	$(TERLC_EXACT_TEST) commands::db::history::history_test::applied_migration_from_postgres_row_rejects_invalid_row_content -- --exact

repl-check:
	$(TERLC_EXACT_TEST) runtime::vm_test::evaluator_applies_lambda_function_value_call -- --exact
	$(TERLC_EXACT_TEST) commands::repl::repl_test::repl_expression_with_bindings_parenthesizes_lambda_binding_values -- --exact

sql-form-check:
	$(PYTHON) tools/check_sql_form_boundary.py
	$(EXACT_CARGO_TEST) -p terlan parser::parser_expr_test::tests::formal_typed_sql_raw_macro_expr_parses_result_type -- --exact
	$(EXACT_CARGO_TEST) -p terlan parser::parser_expr_test::tests::formal_typed_sql_raw_macro_expr_parses_interpolation_expressions -- --exact
	$(EXACT_CARGO_TEST) -p terlan parser::parser_expr_test::tests::formal_typed_sql_raw_macro_expr_rejects_bad_interpolation -- --exact
	$(EXACT_CARGO_TEST) -p terlan parser::parser_expr_test::tests::formal_typed_sql_raw_macro_expr_ignores_comment_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::syntax_output_expr_test::tests::syntax_output_includes_typed_sql_raw_macro_expr_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::syntax_output_expr_test::tests::syntax_output_includes_typed_sql_interpolation_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::syntax_output_expr_test::tests::syntax_output_ignores_typed_sql_comment_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan sql_forms_test::infers_select_limit_one_as_optional_one -- --exact
	$(EXACT_CARGO_TEST) -p terlan sql_forms_test::infers_mutating_statement_without_returning_as_affected_rows -- --exact
	$(EXACT_CARGO_TEST) -p terlan sql_forms_test::rewrites_interpolations_to_postgres_placeholders_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan sql_forms_test::reports_ready_sql_wrapper_lowering_front_door -- --exact
	$(EXACT_CARGO_TEST) -p terlan sql_forms_test::builds_ready_sql_wrapper_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan diagnostic_test::syntax_output_rejects_sql_projection_field_not_on_row_struct -- --exact
	$(EXACT_CARGO_TEST) -p terlan diagnostic_test::syntax_output_uses_sql_wrapper_result_type_for_return_checking -- --exact
	$(EXACT_CARGO_TEST) -p terlan core_lowering_test::syntax_output_lowering_to_core_records_sql_query_payload -- --exact

sql-runtime-check:
	$(TERLC_EXACT_TEST) commands::emit::emit_test::run_emit_writes_sql_runtime_for_typed_sql_forms -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::sql_runtime_test::build_command_emits_sql_runtime_for_typed_sql_forms -- --exact

api-schema-check:
	$(TERLC_EXACT_TEST) compiler::api_contract::api_contract_test::router_source_contract_extracts_routes -- --exact
	$(TERLC_EXACT_TEST) compiler::api_contract::api_contract_test::router_source_contract_projects_to_openapi_paths -- --exact
	$(TERLC_EXACT_TEST) commands::api::mod_test::api_emit_from_source_writes_route_openapi_paths -- --exact
	$(TERLC_EXACT_TEST) commands::api::mod_test::api_import_generates_client_module_and_skip_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::api::mod_test::api_import_records_unsupported_operation_skips -- --exact

runtime-release-dependency-check:
	$(PYTHON) tools/check_runtime_release_dependencies.py

release-0-0-4-preflight:
	$(CARGO) fmt --all -- --check
	$(MAKE) --no-print-directory release-boundary-check
	$(MAKE) --no-print-directory single-root-contract-check
	$(MAKE) --no-print-directory diff-whitespace-check
	$(MAKE) --no-print-directory workspace-version-check
	$(MAKE) --no-print-directory release-version-metadata-check
	$(MAKE) --no-print-directory source-extension-check
	$(MAKE) --no-print-directory rust-quality-check
	$(MAKE) --no-print-directory test-hierarchy-check
	$(MAKE) --no-print-directory cli-exact-selector-check
	$(MAKE) --no-print-directory shared-helper-check
	$(MAKE) --no-print-directory oxc-boundary-check
	$(MAKE) --no-print-directory changelog-public-scope-check
	$(MAKE) --no-print-directory internal-docs-check
	$(MAKE) --no-print-directory module-readme-check
	$(MAKE) --no-print-directory rustdoc-check
	$(MAKE) --no-print-directory cli-check
	$(MAKE) --no-print-directory stdlib-release-check
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_source_and_beam_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_js_module_and_manifest_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_js_std_core_string_intrinsics -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_js_declarations_when_requested -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_browser_web_package_for_js_browser_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_manifest_declared_static_assets_for_js_browser_project -- --exact
	$(MAKE) --no-print-directory browser-package-preflight
	$(TERLC_EXACT_TEST) commands::serve::serve_test::run_serve_check_validates_without_binding_port -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_accepts_manifest_handler -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_rejects_unsafe_handler_route -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_get_and_head -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::beam_ebin_dir_for_web_root_uses_build_root_sibling -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::render_beam_handler_eval_passes_request_map_and_target -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::parse_beam_handler_stdout_accepts_stable_response_protocol -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::beam_handler_response_converts_from_native_http_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::beam_handler_response_rejects_invalid_native_status -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::parse_beam_handler_stdout_rejects_bad_status -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::execute_beam_handler_reports_missing_ebin_before_running_erl -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::execute_beam_handler_reports_missing_beam_before_running_erl -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_rejects_missing_manifest_asset -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_rejects_unsafe_manifest_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::inject_reload_script_inserts_before_body_close -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::reload_sse_response_preserves_live_reload_response_contract -- --exact
	$(TERLC_EXACT_TEST) commands::serve::watch::watch_test::reload_watch_backend_uses_notify -- --exact
	$(TERLC_EXACT_TEST) commands::serve::watch::watch_test::should_reload_for_event_accepts_artifact_changes -- --exact
	$(TERLC_EXACT_TEST) commands::serve::watch::watch_test::broadcast_reload_removes_disconnected_subscribers -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::parse_serve_static_args_preserves_shared_server_settings -- --exact
	$(TERLC_EXACT_TEST) tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::parse_init_args_accepts_web_profile_before_project -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::parse_init_args_accepts_web_profile_after_project -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::write_project_web_profile_creates_browser_and_http_modules -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::next_steps_for_web_profile_build_both_targets_and_serve -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_js_with_oxc_codegen_reprints_module_source -- --exact
	$(TERLC_EXACT_TEST) commands::bind::bind_test::generate_js_dom_bindings_writes_fixture_outputs -- --exact
	$(TERLC_EXACT_TEST) tests::target_profile_test::parse_args_accepts_js_shared_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::target_profile_test::parse_args_accepts_js_target_profile_alias -- --exact
	$(TERLC_EXACT_TEST) tests::target_profile_test::parse_args_accepts_js_browser_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::target_profile_test::parse_args_accepts_js_worker_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::parse_test_args_accepts_explicit_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::release_api_test_modules_have_embedded_runner_support -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::effective_js_test_profile_defaults_to_shared_js_profile -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::validation_pass_report_marks_all_tests_as_validated -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::run_js_tests_writes_validation_manifests -- --exact
	$(MAKE) --no-print-directory js-stdlib-smoke-check
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::embedded_std_interfaces_include_js_std_contracts -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::compile_syntax_module_with_js_profile_resolves_js_string_summary -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::compile_syntax_module_with_browser_profile_resolves_generated_dom_summary -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::diagnostics_test::build_command_rejects_js_std_import_for_erlang_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::diagnostics_test::build_command_rejects_browser_dom_import_for_shared_js_target -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::target_profile_test::tests::std_bridge_test::rejects_js_std_module_for_non_js_profiles -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::target_profile_test::tests::std_bridge_test::rejects_browser_dom_js_std_module_for_shared_js_profile -- --exact

release-0-0-5-preflight: release-0-0-4-preflight
	$(MAKE) --no-print-directory runtime-release-dependency-check
	$(MAKE) --no-print-directory http-runtime-stack-check
	$(MAKE) --no-print-directory editor-check
	$(MAKE) --no-print-directory safenative-postgres-check
	$(MAKE) --no-print-directory safenative-http-cookie-check
	$(MAKE) --no-print-directory safenative-postgres-docker-check
	$(MAKE) --no-print-directory http-router-check
	$(MAKE) --no-print-directory http-observability-check
	$(MAKE) --no-print-directory http-tls-check
	$(MAKE) --no-print-directory web-compose-check
	$(MAKE) --no-print-directory stdlib-data-check
	$(MAKE) --no-print-directory stdlib-db-check
	$(MAKE) --no-print-directory stdlib-http-check
	$(MAKE) --no-print-directory stdlib-log-check
	$(MAKE) --no-print-directory stdlib-sync-check
	$(MAKE) --no-print-directory template-contract-check
	$(MAKE) --no-print-directory artifact-template-check
	$(MAKE) --no-print-directory private-field-check
	$(MAKE) --no-print-directory db-command-check
	$(MAKE) --no-print-directory static-command-check
	$(MAKE) --no-print-directory repl-check
	$(MAKE) --no-print-directory sql-form-check
	$(MAKE) --no-print-directory sql-runtime-check
	$(MAKE) --no-print-directory web-profile-preflight
	$(MAKE) --no-print-directory static-profile-preflight
	$(MAKE) --no-print-directory static-docs-check
formal-cli-phase-contract-gate:
	$(TERLC_EXACT_TEST) tests::run_phase_contract_fixtures_backend_parity -- --exact
	$(TERLC_EXACT_TEST) tests::run_phase_contract_fixtures_match_golden -- --exact
	$(TERLC_EXACT_TEST) tests::interface_test::run_interface_success_and_error_paths -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact

formal-cli-build-gate:
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_package_name_with_default_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_explicit_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_missing_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_missing_package_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_invalid_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_invalid_package_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_artifact_kind -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_accepts_reserved_empty_dependency_sections -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_dependency_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_erlang_package_adapter_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_erlang_package_adapter -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_registry_dependency_in_local_scope -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_wrong_target_dependency_source -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_dependency_without_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_section -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_source_and_beam_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_sources_and_beams_for_directory -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_sources_and_beams_for_recursive_package_layout -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_compiles_recursive_type_and_value_import_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_rejects_project_manifest_before_silent_directory_scan -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_compiles_project_manifest_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_rejects_project_source_outside_package_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_receiver_method_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_preserves_erlang_package_adapter_metadata_without_rebar3_files -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_compiles_project_manifest_multiple_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_compiles_project_with_local_path_dependency -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_local_path_dependency_without_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_local_path_dependency_cycle -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_hex_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_npm_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_cargo_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_compiles_directory_with_imported_constructors_and_aliases -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_compiles_directory_with_aliased_imported_alias_patterns -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_compiles_directory_with_aliased_imported_alias_constructor_chain -- --exact

formal-cli-a0-54-constructor-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-12-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory formal-erlang-a0-15-gate
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_constructor_edge_cases_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_rejects_public_constructor_returning_private_type -- --exact

formal-cli-a0-55-function-clause-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-23-keyword-gate
	$(MAKE) --no-print-directory formal-syntax-a0-26-declaration-gate
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_function_clause_edge_cases_before_phase_manifest -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_refines_function_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_function_clause_summaries -- --exact

formal-cli-a0-56-primary-expression-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-24-collection-gate
	$(EXACT_CARGO_TEST) -p terlan parser::tests::formal_macro_expr_parses_as_primary_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan parser::tests::formal_raw_macro_expr_requires_immediate_raw_block -- --exact
	$(EXACT_CARGO_TEST) -p terlan parser::tests::formal_constructor_chain_expr_parses_with_record_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_quoted_atom_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_sequence_primary_expr_trees -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_raw_macro_primary_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_index_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact

formal-cli-a0-57-keyword-expression-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-3-gate
	$(MAKE) --no-print-directory formal-erlang-a0-4-gate
	$(MAKE) --no-print-directory formal-syntax-a0-23-keyword-gate
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_allows_keyword_expressions_in_operator_chains -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_if_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_receive_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_receive_after_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_try_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_checks_if_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_checks_receive_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_checks_try_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_supports_try_after_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_supports_receive_after_timeout -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_if_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_receive_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_try_core_expr -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_receive_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_try_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_quote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_unquote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_partial_case_branch_for_core_v0_target_profile -- --exact

formal-cli-a0-58-calls-and-references-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-10-gate
	$(MAKE) --no-print-directory formal-erlang-a0-16-gate
	$(MAKE) --no-print-directory formal-erlang-a0-17-gate
	$(MAKE) --no-print-directory formal-erlang-a0-19-gate
	$(MAKE) --no-print-directory formal-erlang-a0-20-gate
	$(MAKE) --no-print-directory formal-erlang-a0-21-diagnostic-gate
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_infers_local_calls_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_infers_field_access_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_local_call_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_function_value_call_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_typechecks_pipe_into_function_value_call -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_index_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_field_access_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_marks_remote_call_proof_model_required -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_rejects_remote_fun_ref_source_syntax -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_resolves_local_receiver_method_calls_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_rejects_duplicate_receiver_method_identity_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_rejects_receiver_methods_for_imported_owner_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_fun_call_for_a0_16_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_fun_call_out_of_a0_15_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_qualified_calls_for_a0_20_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_qualified_calls_out_of_a0_19_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_rejects_method_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_receiver_method_entrypoint -- --exact

formal-cli-a0-59-data-form-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-7-gate
	$(MAKE) --no-print-directory formal-erlang-a0-8-gate
	$(MAKE) --no-print-directory formal-erlang-a0-9-gate
	$(MAKE) --no-print-directory formal-syntax-a0-24-collection-gate
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_preserves_binary_segment_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_record_suffix_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_binds_list_comprehension_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_rejects_list_comprehension_non_list_source_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_binary_literal -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_map_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_list_cons_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_fixed_array_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_list_comprehension_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_record_construct_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_record_access_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_record_update_expr -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_lists_for_a0_7_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_lists_out_of_a0_6_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_binary_for_a0_8_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_binary_out_of_a0_7_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_list_cons_for_a0_9_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_list_cons_out_of_a0_8_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_multi_generator_list_comprehension_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_binary_segment_lowering_in_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact

formal-cli-a0-60-pattern-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-4-gate
	$(MAKE) --no-print-directory formal-erlang-a0-5-gate
	$(MAKE) --no-print-directory formal-erlang-a0-6-gate
	$(MAKE) --no-print-directory formal-erlang-a0-7-gate
	$(MAKE) --no-print-directory formal-erlang-a0-9-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory formal-syntax-a0-25-pattern-gate
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_recursive_expression_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_case_guard_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_declared_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_unknown_constructor_patterns_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_list_cons_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_single_shape_alias_constructor_patterns_report_arity_mismatch_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_union_aliases_do_not_generate_constructor_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_binds_case_constructor_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_refines_case_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_record_pattern_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_includes_float_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_includes_map_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_resolves_imported_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_resolves_aliased_imported_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_resolves_local_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_resolves_imported_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_case_with_record_pattern_requires_proof_model -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_map_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_record_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_float_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_aliased_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::run_check_single_file_rejects_aliased_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_direct_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_aliased_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_declared_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::run_check_single_file_rejects_local_unknown_constructor_pattern_before_core_phase -- --exact

formal-cli-a0-61-lexical-and-name-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-5-gate
	$(MAKE) --no-print-directory formal-erlang-a0-10-gate
	$(MAKE) --no-print-directory formal-erlang-a0-12-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory parser-fixture-check
	$(EXACT_CARGO_TEST) -p terlan parser::tests::formal_raw_atom_patterns_are_literal_patterns -- --exact
	$(EXACT_CARGO_TEST) -p terlan parser::tests::formal_nullary_constructor_pattern_call_is_rejected -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_quoted_atom_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_normalizes_prefixed_integer_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_keeps_constructor_call_candidates_as_named_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_imported_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_literal_aliases_compare_with_literal_values_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_remote_literal_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_imported_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_remote_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_compound_core_type_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_records_type_decl_core_body_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_float_literal -- --exact
	$(EXACT_CARGO_TEST) -p terlan tests::syntax_output_lowering_to_core_binary_literal -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_raw_atoms_for_a0_5_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_raw_atoms_out_of_a0_4_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_named_call_for_a0_10_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_named_call_out_of_a0_9_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_constructor_call_for_a0_12_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_constructor_call_out_of_a0_11_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact

formal-cli-js-gate:
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_uses_core_function_exports -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_integer_division -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_js_with_oxc_codegen_reprints_module_source -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_emits_core_surface -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_minimal_direct_oxc_ast_module_prints_export -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_arithmetic_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_integer_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_float_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_string_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_bool_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_total_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_partial_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_partial_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_guarded_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_destructuring_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_lambda_value -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_simple_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_destructuring_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_remote_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_rejects_remote_fun_ref_source_syntax -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_constructor_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_constructor_chain -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_try_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_quote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_unquote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_html_block_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_array_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_unary_negation -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_list_cons -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_index_trait_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_map_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_field_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_record_construct -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_record_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_record_update -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_template_instantiate -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_binary_operator_set -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_contains_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_starts_with_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_length_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_emits_named_call_private_helper -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_ignores_unreachable_private_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_uses_direct_reachability_filter -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_binding_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_typescript_declarations_uses_core_surface -- --exact
	$(TERLC_EXACT_TEST) tests::emit_js_test::run_emit_js_reports_errors -- --exact
	$(TERLC_EXACT_TEST) tests::emit_js_test::run_emit_js_writes_js_and_declarations -- --exact

formal-cli-rust-gate:
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_uses_core_function_visibility -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_pipe_forward_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_handles_function_value_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_contains_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_starts_with_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_length_intrinsic_probe -- --exact

formal-cli-doc-gate:
	$(TERLC_EXACT_TEST) tests::doc_test::formal_doc_markdown_generates_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::doc_test::formal_doctest_compiles_terlan_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_html_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_markdown_html_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_syntax_output_discovers_entrypoints_and_routes -- --exact

formal-cli-a0-50-template-frontend-gate:
	$(TERLC_EXACT_TEST) commands::artifacts::artifacts_test::collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact

formal-cli-a0-62-template-boundary-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-43-template-raw-gate
	$(MAKE) --no-print-directory formal-cli-a0-50-template-frontend-gate
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan syntax_output::tests::syntax_output_includes_struct_constructor_trait_and_template_signatures -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_unresolved_template_body_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact

formal-incremental-gate:
	$(TERLC_EXACT_TEST) tests::check_phase_test::run_check_dir_rejects_module_layout_mismatch -- --exact
	$(TERLC_EXACT_TEST) tests::check_incremental_test::run_check_dir_incremental_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) tests::check_incremental_test::run_check_dir_incremental_with_trait_interfaces -- --exact

formal-phase-gate:
	@tmpdir=$$(mktemp -d); \
	tmp2=$$(mktemp -d); \
	manifest1=$${tmpdir}/phase-a.json; \
	manifest2=$${tmp2}/phase-b.json; \
	out1=$${tmpdir}/gen1; \
	out2=$${tmp2}/gen2; \
	mkdir -p "$${out1}" "$${out2}"; \
	$(TERLC) check tests/fixtures/mathx.terl --emit-phase-manifest "$${manifest1}"; \
	$(TERLC) check tests/fixtures/mathx.terl --emit-phase-manifest "$${manifest2}"; \
	cmp "$${manifest1}" "$${manifest2}" >/dev/null; \
	$(TERLC) emit tests/fixtures/mathx.terl --out-dir "$${out1}"; \
	$(TERLC) emit tests/fixtures/mathx.terl --out-dir "$${out2}"; \
	diff -qr "$${out1}" "$${out2}" >/dev/null; \
	rm -rf "$${tmpdir}" "$${tmp2}"

formal-directory-phase-gate:
	@tmpdir=$$(mktemp -d); \
	cache_a=$${tmpdir}/cache-a; \
	cache_b=$${tmpdir}/cache-b; \
	manifest_a=$${tmpdir}/manifests-a; \
	manifest_b=$${tmpdir}/manifests-b; \
	mkdir -p "$${cache_a}" "$${cache_b}" "$${manifest_a}" "$${manifest_b}"; \
	$(TERLC) check tests/fixtures/phase_contract --cache-dir "$${cache_a}" --emit-phase-manifest "$${manifest_a}"; \
	$(TERLC) check tests/fixtures/phase_contract --cache-dir "$${cache_b}" --emit-phase-manifest "$${manifest_b}"; \
	diff -qr "$${manifest_a}" "$${manifest_b}" >/dev/null; \
	rm -rf "$${tmpdir}"
