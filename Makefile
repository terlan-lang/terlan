CARGO := cargo
PYTHON := python3 -B
SHELL := bash
.SHELLFLAGS := -eo pipefail -c

.PHONY: check test test-release build release-artifact-linux release-artifact-smoke publish-preflight publish validate-ebnf workspace-version-check release-version-metadata-check source-extension-check release-boundary-check single-root-contract-check diff-whitespace-check rust-quality-check test-hierarchy-check cli-exact-selector-check shared-helper-check oxc-boundary-check http-runtime-stack-check runtime-release-dependency-self-test changelog-public-scope-check internal-docs-check module-readme-check rustdoc-check clean

include crates/terlan/cli.mk
include std/stdlib.mk
include editors/editor.mk

ifneq ($(filter publish publish-preflight,$(MAKECMDGOALS)),)
ifndef VERSION
$(error VERSION is required. Use: make $(firstword $(MAKECMDGOALS)) VERSION=<release-version>)
endif
ifneq ($(filter v%,$(VERSION)),)
$(error VERSION must not include the leading v. Use: make $(firstword $(MAKECMDGOALS)) VERSION=$(patsubst v%,%,$(VERSION)))
endif
endif

check:
	$(MAKE) release-boundary-check
	$(MAKE) single-root-contract-check
	$(MAKE) diff-whitespace-check
	$(MAKE) workspace-version-check
	$(MAKE) release-version-metadata-check
	$(MAKE) source-extension-check
	$(MAKE) rust-quality-check
	$(MAKE) test-hierarchy-check
	$(MAKE) cli-exact-selector-check
	$(MAKE) shared-helper-check
	$(MAKE) oxc-boundary-check
	$(MAKE) http-tls-check
	$(MAKE) http-runtime-stack-check
	$(MAKE) runtime-release-dependency-self-test
	$(MAKE) changelog-public-scope-check
	$(MAKE) internal-docs-check
	$(MAKE) module-readme-check
	$(MAKE) rustdoc-check
	$(MAKE) cli-check
	$(MAKE) stdlib-check
	$(MAKE) editor-check
	$(MAKE) lsp-check
	$(MAKE) api-schema-check
	$(PYTHON) tools/validate_ebnf.py --strict

test:
	$(MAKE) cli-test

test-release:
	$(MAKE) cli-test-release
	$(MAKE) stdlib-release-check

build:
	$(MAKE) cli-build

validate-ebnf:
	$(PYTHON) tools/validate_ebnf.py --strict

workspace-version-check:
	bash scripts/check_workspace_version_inheritance.sh

release-version-metadata-check:
	bash scripts/check_release_version_metadata.sh

source-extension-check:
	bash scripts/check_terlan_source_extensions.sh

release-boundary-check:
	bash scripts/check_release_boundary.sh

single-root-contract-check:
	$(PYTHON) tools/check_single_root_contract.py

diff-whitespace-check:
	git diff --check

rust-quality-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-quality

test-hierarchy-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- test-hierarchy

cli-exact-selector-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- cli-exact-selectors

shared-helper-check:
	$(PYTHON) tools/check_shared_helpers.py

oxc-boundary-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- oxc-boundary

http-runtime-stack-check:
	$(PYTHON) tools/check_http_runtime_stack.py
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_http_request_body_json_direct_erlang_lowering -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_static_get_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_static_file_with_query_string -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_omits_static_head_response_body -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_static_parent_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_unmatched_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_streams_reload_sse_events -- --exact

runtime-release-dependency-self-test:
	$(PYTHON) tools/check_runtime_release_dependencies.py --self-test

changelog-public-scope-check:
	$(PYTHON) tools/check_changelog_public_scope.py

internal-docs-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- internal-docs

module-readme-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- module-readmes

rustdoc-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-docs

release-artifact-linux:
	$(MAKE) release-boundary-check
	$(MAKE) release-version-metadata-check
	$(MAKE) source-extension-check
	$(MAKE) cli-release-artifact-linux
	$(MAKE) release-artifact-smoke

release-artifact-smoke:
	test -x dist/terlc
	test -f dist/terlc-linux-x86_64.tar.gz
	tmpdir=$$(mktemp -d /tmp/terlan-release-artifact-smoke.XXXXXX); \
	trap 'rm -rf "$$tmpdir"' EXIT; \
	expected_version=$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1); \
	tar -xzf dist/terlc-linux-x86_64.tar.gz -C "$$tmpdir"; \
	"$$tmpdir/terlc" --version | grep -Fx "terlc $$expected_version"; \
	"$$tmpdir/terlc" init "$$tmpdir/hello" --profile web; \
	printf '%s\n' 'hello asset' > "$$tmpdir/hello/assets/hello.txt"; \
	"$$tmpdir/terlc" --out-dir "$$tmpdir/hello/_build" build "$$tmpdir/hello" --target erlang; \
	"$$tmpdir/terlc" --target-profile js.browser --out-dir "$$tmpdir/hello/_build" build "$$tmpdir/hello" --target js.browser; \
	test -f "$$tmpdir/hello/_build/web/assets/hello.txt"; \
	"$$tmpdir/terlc" serve "$$tmpdir/hello/_build/web" --check

publish-preflight:
	@echo "Preparing Terlan $(VERSION) publication preflight"
	@if [ -n "$$(git status --porcelain)" ]; then \
		changed_count=$$(git status --porcelain | wc -l | tr -d ' '); \
		echo "publish-preflight failed: working tree is not clean"; \
		echo "changed files: $$changed_count"; \
		echo "first changed files:"; \
		git status --short | sed -n '1,20p'; \
		if [ "$$changed_count" -gt 20 ]; then \
			echo "... $$((changed_count - 20)) more changed files omitted"; \
		fi; \
		echo "next step: review and commit the release contents, then rerun make publish VERSION=$(VERSION)"; \
		exit 1; \
	fi
	@branch=$$(git branch --show-current); \
	if [ "$$branch" != "main" ]; then \
		echo "publication must run from main; current branch is $$branch"; \
		exit 1; \
	fi
	bash scripts/check_release_version_metadata.sh "$(VERSION)"
	@if git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		tag_sha=$$(git rev-parse "refs/tags/v$(VERSION)"); \
		head_sha=$$(git rev-parse HEAD); \
		if [ "$$tag_sha" != "$$head_sha" ]; then \
			echo "local tag v$(VERSION) already exists at $$tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "local tag v$(VERSION) already exists at HEAD; continuing"; \
	fi
	@if git ls-remote --exit-code --tags origin "refs/tags/v$(VERSION)" >/dev/null 2>&1; then \
		echo "remote tag v$(VERSION) already exists"; \
		exit 1; \
	fi
	@if [ "$(VERSION)" = "0.0.5" ]; then \
		$(MAKE) check; \
		$(MAKE) test-release; \
		$(MAKE) release-0-0-5-preflight; \
		$(MAKE) release-artifact-linux; \
	elif [ "$(VERSION)" = "0.0.4" ]; then \
		$(MAKE) check; \
		$(MAKE) test-release; \
		$(MAKE) release-0-0-4-preflight; \
		$(MAKE) release-artifact-linux; \
	else \
		$(MAKE) check; \
		$(MAKE) test-release; \
	fi

publish: publish-preflight
	@if ! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		git tag "v$(VERSION)"; \
	fi
	git push origin main
	git push origin "v$(VERSION)"
	@echo "Published tag v$(VERSION). GitHub Actions will build and upload the release artifact."

clean:
	$(MAKE) cli-clean
