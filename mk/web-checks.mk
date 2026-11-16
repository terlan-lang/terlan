# ---- Web/HTTP/Angular integration checks ----

# Runs the quality gate for the web asset build pipeline
web-asset-pipeline-check: \
	typed-template-render-mode-check \
	browser-package-preflight \
	web-profile-preflight
	$(TERLAN_QUALITY) web-asset-pipeline


# Tests HTTP session actor lookup, cookie rotation, expiration, and recovery policy
http-session-actor-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::session_actor_fixtures::http_session_lookup_creates_actor_and_sticky_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::session_actor_fixtures::http_session_reuses_actor_and_table_state_for_cookie_lookup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_rotate_changes_cookie_without_losing_actor_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_expiration_cleans_actor_table_and_reports_stale -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_recovery_policy_can_fail_closed_for_stale_cookie -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_rejects_invalid_runtime_configuration -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::http_session::http_session_test


# Checks the HTTP runtime stack's static, SSE, and routing behavior
http-runtime-stack-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/HttpRuntimeStackTest.terl
	$(TERLC_EXACT_TEST) commands::serve::serve_test::arguments_and_fixtures::hyper_request_handler_serves_static_get_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::arguments_and_fixtures::hyper_request_handler_serves_static_file_with_query_string -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_omits_static_head_response_body -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_static_parent_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_unmatched_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_streams_reload_sse_events -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_heads_reload_sse_without_opening_stream -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_reload_sse_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::run_serve_check_rejects_dynamic_handlers_missing_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::run_serve_check_rejects_dynamic_handlers_with_removed_beam_runtime -- --exact


# Checks Angular TypeScript integration with the Terlan app
angular-ts-terlan-integration-check: angular-ts-namespace-generation-check angular-ts-terlan-app-ownership-check
	TERLAN_ANGULAR_TS_ROOT="$(CURDIR)" \
	TERLAN_ANGULAR_TS_MODE=integration \
		target/debug/terlc test scripts/self_validation/AngularTsIntegrationTest.terl \
			--name selected_angular_ts_integration_holds


# Checks Angular TypeScript namespace generation
angular-ts-namespace-generation-check:
	TERLAN_ANGULAR_TS_ROOT="$(CURDIR)" \
	TERLAN_ANGULAR_TS_MODE=namespace \
		target/debug/terlc test scripts/self_validation/AngularTsIntegrationTest.terl \
			--name selected_angular_ts_integration_holds


# Checks Angular TypeScript app-ownership boundaries
angular-ts-terlan-app-ownership-check:
	TERLAN_ANGULAR_TS_ROOT="$(CURDIR)" \
	TERLAN_ANGULAR_TS_MODE=app-ownership \
		target/debug/terlc test scripts/self_validation/AngularTsIntegrationTest.terl \
			--name selected_angular_ts_integration_holds


# Validates the web service foundation's contract in Rust and Terlan
web-service-foundation-contract-check: terlan-compiler-bootstrap
	$(CARGO) test -p terlan-service-foundation
	target/debug/terlc check std/service/ServiceFoundationTest.terl
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- contract


# Validates the web service foundation's runtime behavior
web-service-foundation-runtime-check: terlan-compiler-bootstrap
	$(CARGO) test -p terlan --lib service_foundation::tests
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- runtime


# Validates the web service foundation's adapter layer
web-service-foundation-adapter-check: terlan-compiler-bootstrap
	$(CARGO) test -p terlan-foundations-adapter
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- adapter


# Validates the web service foundation's pruning behavior
web-service-foundation-pruning-check: terlan-compiler-bootstrap
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- pruning


# Runs all web service foundation checks, then reports the combined result
web-service-foundation-check: web-service-foundation-contract-check web-service-foundation-runtime-check web-service-foundation-adapter-check web-service-foundation-pruning-check
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- report


