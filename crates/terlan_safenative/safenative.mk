# Terlan SafeNative crate targets.
#
# This file is included by the root Makefile. SafeNative-specific release gates
# live with the crate so runtime bridge checks do not accumulate in the CLI
# makefile.

.PHONY: safenative-help safenative-postgres-check safenative-postgres-docker-check safenative-http-cookie-check safenative-http-postgres-handler-check

safenative-help:
	@echo "  make safenative-postgres-check - run SafeNative Postgres bridge contract checks"
	@echo "  make safenative-postgres-docker-check - run Docker-backed live Postgres adapter checks"
	@echo "  make safenative-http-cookie-check - run SafeNative HTTP cookie contract checks"
	@echo "  make safenative-http-postgres-handler-check - run opt-in HTTP handler/Postgres runtime integration check"

safenative-postgres-check:
	$(CARGO) test -p terlan_safenative metadata -- --nocapture
	$(CARGO) test -p terlan_safenative postgres -- --nocapture

safenative-http-cookie-check:
	$(CARGO) test -p terlan_safenative http::http_test::request_cookies_returns_mutable_cookie_jar -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::request_cookie_header_parser_splits_request_cookie_pairs -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::request_cookie_jar_rejects_invalid_mutations_without_recording_them -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::response_applies_cookie_jar_mutations_in_order -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_set_header_serializes_supported_attributes -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_set_header_with_options_serializes_full_option_surface -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_set_header_with_options_serializes_same_site_variants -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_delete_header_serializes_expiring_cookie -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_set_header_rejects_invalid_names -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_set_header_rejects_invalid_values_and_paths -- --exact --nocapture
	$(CARGO) test -p terlan_safenative http::http_test::cookie_set_header_with_options_rejects_invalid_optional_attributes -- --exact --nocapture
	$(CARGO) test -p terlan_safenative runtime::runtime_test::runtime_executes_http_cookie_jar_operations_through_terms -- --exact --nocapture

safenative-http-postgres-handler-check:
	$(CARGO) test -p terlan_safenative runtime::runtime_test::runtime_executes_full_cycle_http_postgres_handler_when_configured -- --exact --nocapture

safenative-postgres-docker-check:
	@echo "safenative-postgres-docker-check validates live Rust/Tokio Postgres adapter and DB migration command execution against a disposable database."
	@if [ -n "$${TERLAN_TEST_POSTGRES_URL:-}" ]; then \
		echo "safenative-postgres-docker-check owns TERLAN_TEST_POSTGRES_URL; unset it before running this disposable-database gate"; \
		exit 1; \
	fi
	@command -v docker >/dev/null 2>&1 || { echo "safenative-postgres-docker-check requires Docker"; exit 1; }
	@docker info >/dev/null 2>&1 || { echo "safenative-postgres-docker-check cannot connect to the Docker daemon; start Docker and ensure the current user can access /var/run/docker.sock"; exit 1; }
	@set -eu; \
	image="$${TERLAN_POSTGRES_DOCKER_IMAGE:-postgres:16-alpine}"; \
	container="terlan-postgres-check-$$(date +%s)-$$$$"; \
	echo "Starting $$image as $$container"; \
	docker run --rm -d \
		--name "$$container" \
		-e POSTGRES_USER=terlan \
		-e POSTGRES_PASSWORD=terlan \
		-e POSTGRES_DB=terlan_test \
		-p 127.0.0.1::5432 \
		--health-cmd='pg_isready -U terlan -d terlan_test' \
		--health-interval=1s \
		--health-timeout=5s \
		--health-retries=30 \
		"$$image" >/dev/null; \
	trap 'docker rm -f "$$container" >/dev/null 2>&1 || true' EXIT; \
	for attempt in $$(seq 1 45); do \
		status="$$(docker inspect -f '{{.State.Health.Status}}' "$$container" 2>/dev/null || true)"; \
		if [ "$$status" = "healthy" ]; then \
			break; \
		fi; \
		if [ "$$attempt" = "45" ]; then \
			echo "Postgres Docker container did not become healthy"; \
			docker logs "$$container" || true; \
			exit 1; \
		fi; \
		sleep 1; \
	done; \
	port="$$(docker port "$$container" 5432/tcp | sed -n 's/.*://p' | head -n 1)"; \
	url="postgres://terlan:terlan@127.0.0.1:$$port/terlan_test"; \
		echo "Validating Postgres container at 127.0.0.1:$$port"; \
		docker exec "$$container" psql -U terlan -d terlan_test -c 'SELECT 1;' >/dev/null; \
		TERLAN_TEST_POSTGRES_URL="$$url" TERLAN_DATABASE_URL="$$url" $(CARGO) test -p terlan_safenative postgres -- --nocapture; \
		TERLAN_TEST_POSTGRES_URL="$$url" TERLAN_DATABASE_URL="$$url" $(CARGO) test -p terlan_safenative runtime::runtime_test::runtime_executes_full_cycle_http_postgres_handler_when_configured -- --exact --nocapture; \
		TERLAN_TEST_POSTGRES_URL="$$url" TERLAN_DATABASE_URL="$$url" $(EXACT_CARGO_TEST) -p terlan_cli commands::db::mod_test::run_db_migration_lifecycle_against_live_postgres_when_configured -- --exact --nocapture
