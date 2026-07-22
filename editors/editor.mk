# Terlan editor integration validation targets.
#
# This file is included by the repository root Makefile so editor checks stay
# owned by the editor package layout while remaining available as root targets.

NPM_PACK_CACHE ?= /tmp/terlan-npm-cache
TREE_SITTER_CLI_HOME ?= /tmp/terlan-tree-sitter-home
TREE_SITTER_CLI_CACHE ?= /tmp/terlan-tree-sitter-cache

.PHONY: editor-help editor-check lsp-outline-check editor-code-action-auto-import-check editor-completion-signature-check editor-runnable-debug-launch-check editor-semantic-token-icon-check editor-diagnostic-parity-check editor-extension-install-update-check vscode-extension-check tree-sitter-package-check tree-sitter-cli-check neovim-editor-check emacs-editor-check intellij-editor-check shared-editor-icon-check shared-editor-contract-check editor-debugger-surface-check
.PHONY: editor-definition-navigation-check

editor-help:
	@echo "  make editor-check - verify editor package contracts"
	@echo "  make lsp-outline-check - run Terlan LSP document-symbol outline regressions"
	@echo "  make editor-code-action-auto-import-check - run LSP auto-import quick-fix regressions"
	@echo "  make editor-completion-signature-check - run LSP completion, signature-help, and inlay-hint regressions"
	@echo "  make editor-runnable-debug-launch-check - run editor runnable/debug launch contract checks"
	@echo "  make editor-semantic-token-icon-check - run editor highlighting and icon contract checks"
	@echo "  make editor-diagnostic-parity-check - run editor/compiler diagnostic parity checks"
	@echo "  make editor-extension-install-update-check - run editor package install/update artifact checks"
	@echo "  make vscode-extension-check - run VS Code extension syntax and smoke tests"
	@echo "  make tree-sitter-package-check - run Tree-sitter package metadata smoke"
	@echo "  make tree-sitter-cli-check - run Tree-sitter generate/test with local package deps"
	@echo "  make neovim-editor-check - run Neovim editor package smoke"
	@echo "  make emacs-editor-check - run Emacs editor package smoke"
	@echo "  make intellij-editor-check - run IntelliJ-family editor package smoke"
	@echo "  make shared-editor-icon-check - run shared editor icon smoke"
	@echo "  make shared-editor-contract-check - run cross-editor suffix and LSP contract smoke"
	@echo "  make editor-debugger-surface-check - run editor debugger surface contract smoke"

editor-check: lsp-outline-check vscode-extension-check tree-sitter-package-check neovim-editor-check emacs-editor-check intellij-editor-check shared-editor-icon-check shared-editor-contract-check editor-runnable-debug-launch-check editor-semantic-token-icon-check editor-diagnostic-parity-check editor-extension-install-update-check editor-debugger-surface-check

lsp-outline-check:
	$(RUST_TEST) --locked -p terlan --features editor-lsp --bin terlan-lsp document_symbol -- --nocapture

editor-definition-navigation-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- editor-definition-navigation-report

editor-code-action-auto-import-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- editor-code-action-auto-import-report

editor-completion-signature-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- editor-completion-signature-report

editor-runnable-debug-launch-check:
	node editors/shared/test/runnable_debug_launch_test.js
	node editors/shared/test/debugger_surface_test.js

editor-semantic-token-icon-check:
	node editors/shared/test/semantic_token_icon_report_test.js
	node tree-sitter-terlan/test/package_smoke_test.js
	node editors/vscode/test/textmate_bridge_test.js
	node editors/shared/test/icon_smoke_test.js

editor-diagnostic-parity-check:
	node editors/shared/test/diagnostic_parity_report_test.js
	node editors/vscode/test/diagnostics_smoke_test.js

editor-extension-install-update-check:
	mkdir -p $(NPM_PACK_CACHE)
	cd editors/vscode && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >/tmp/terlan-vscode-pack.json
	node editors/vscode/test/pack_dry_run_test.js /tmp/terlan-vscode-pack.json
	cd tree-sitter-terlan && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >/tmp/terlan-tree-sitter-pack.json
	node tree-sitter-terlan/test/pack_dry_run_test.js /tmp/terlan-tree-sitter-pack.json
	node editors/shared/test/extension_install_update_report_test.js /tmp/terlan-vscode-pack.json /tmp/terlan-tree-sitter-pack.json

vscode-extension-check:
	mkdir -p $(NPM_PACK_CACHE)
	cd editors/vscode && npm_config_cache=$(NPM_PACK_CACHE) npm run check && npm test && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >/tmp/terlan-vscode-pack.json && node test/pack_dry_run_test.js /tmp/terlan-vscode-pack.json

tree-sitter-package-check:
	mkdir -p $(NPM_PACK_CACHE)
	cd tree-sitter-terlan && npm_config_cache=$(NPM_PACK_CACHE) npm run check && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >/tmp/terlan-tree-sitter-pack.json && node test/pack_dry_run_test.js /tmp/terlan-tree-sitter-pack.json

tree-sitter-cli-check:
	@if [ ! -x tree-sitter-terlan/node_modules/.bin/tree-sitter ]; then \
		echo "tree-sitter-cli-check requires local Tree-sitter package dependencies"; \
		echo "run: npm install --prefix tree-sitter-terlan --no-audit --no-fund"; \
		exit 127; \
	fi
	mkdir -p $(TREE_SITTER_CLI_HOME) $(TREE_SITTER_CLI_CACHE)
	cd tree-sitter-terlan && HOME=$(TREE_SITTER_CLI_HOME) XDG_CACHE_HOME=$(TREE_SITTER_CLI_CACHE) npm run check:cli

neovim-editor-check:
	node editors/neovim/test/package_smoke_test.js

emacs-editor-check:
	node editors/emacs/test/package_smoke_test.js

intellij-editor-check:
	node editors/intellij/test/package_smoke_test.js

shared-editor-icon-check:
	node editors/shared/test/icon_smoke_test.js

shared-editor-contract-check:
	node editors/shared/test/editor_contract_test.js

editor-debugger-surface-check:
	node editors/shared/test/debugger_surface_test.js
