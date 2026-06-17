CARGO := cargo

.PHONY: check test build release-artifact-linux publish-preflight publish validate-ebnf workspace-version-check source-extension-check clean

include crates/terlan_cli/cli.mk
include std/stdlib.mk

ifneq ($(filter publish publish-preflight,$(MAKECMDGOALS)),)
ifndef VERSION
$(error VERSION is required. Use: make $(firstword $(MAKECMDGOALS)) VERSION=0.0.3)
endif
ifneq ($(filter v%,$(VERSION)),)
$(error VERSION must not include the leading v. Use: make $(firstword $(MAKECMDGOALS)) VERSION=$(patsubst v%,%,$(VERSION)))
endif
endif

check:
	$(MAKE) cli-check
	$(MAKE) stdlib-check
	python3 tools/validate_ebnf.py --strict

test:
	$(MAKE) cli-test

build:
	$(MAKE) cli-build

validate-ebnf:
	python3 tools/validate_ebnf.py --strict

workspace-version-check:
	bash scripts/check_workspace_version_inheritance.sh

source-extension-check:
	bash scripts/check_terlan_source_extensions.sh

release-artifact-linux:
	$(MAKE) cli-release-artifact-linux

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
	@actual=$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1); \
	if [ "$$actual" != "$(VERSION)" ]; then \
		echo "workspace package version $$actual != $(VERSION)"; \
		exit 1; \
	fi
	@grep -q 'VERSION="$${TERLAN_VERSION:-v$(VERSION)}"' install.sh || { \
		echo "install.sh default version is not v$(VERSION)"; \
		exit 1; \
	}
	@grep -q '^## $(VERSION)$$' CHANGELOG.md || { \
		echo "CHANGELOG.md is missing section ## $(VERSION)"; \
		exit 1; \
	}
	@grep -Eq '^Current version: `$(VERSION)`\.?$$' README.md || { \
		echo "README.md current version is not $(VERSION)"; \
		exit 1; \
	}
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
	$(MAKE) check
	$(MAKE) test

publish: publish-preflight
	@if ! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		git tag "v$(VERSION)"; \
	fi
	git push origin main
	git push origin "v$(VERSION)"
	@echo "Published tag v$(VERSION). GitHub Actions will build and upload the release artifact."

clean:
	$(MAKE) cli-clean
