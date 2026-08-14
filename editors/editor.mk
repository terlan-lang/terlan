# Terlan editor integration validation targets.
#
# This file is included by the repository root Makefile so editor checks stay
# owned by the editor package layout while remaining available as root targets.

NPM_PACK_CACHE ?= $(CURDIR)/target/tmp/npm-cache
TREE_SITTER_CLI_HOME ?= $(CURDIR)/target/tmp/tree-sitter-home
TREE_SITTER_CLI_CACHE ?= $(CURDIR)/target/tmp/tree-sitter-cache
NPM_PACK_OUTPUT_DIR ?= $(CURDIR)/target/tmp/npm-pack
VSCODE_PACK_REPORT := $(NPM_PACK_OUTPUT_DIR)/terlan-vscode-pack.json
TREE_SITTER_PACK_REPORT := $(NPM_PACK_OUTPUT_DIR)/terlan-tree-sitter-pack.json
TREE_SITTER_DEPENDENCY_STAMP := tree-sitter-terlan/node_modules/.terlan-package-lock.stamp

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
	$(RUST_TEST) --locked -p terlan --features editor-lsp --lib document_symbol -- --nocapture

editor-definition-navigation-check:
	$(TERLAN_QUALITY) editor-definition-navigation-report

editor-code-action-auto-import-check:
	$(TERLAN_QUALITY) editor-code-action-auto-import-report

editor-completion-signature-check:
	$(TERLAN_QUALITY) editor-completion-signature-report

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
	mkdir -p $(NPM_PACK_CACHE) $(NPM_PACK_OUTPUT_DIR)
	cd editors/vscode && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >$(VSCODE_PACK_REPORT)
	node editors/vscode/test/pack_dry_run_test.js $(VSCODE_PACK_REPORT)
	cd tree-sitter-terlan && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >$(TREE_SITTER_PACK_REPORT)
	node tree-sitter-terlan/test/pack_dry_run_test.js $(TREE_SITTER_PACK_REPORT)
	node editors/shared/test/extension_install_update_report_test.js $(VSCODE_PACK_REPORT) $(TREE_SITTER_PACK_REPORT)

vscode-extension-check:
	mkdir -p $(NPM_PACK_CACHE) $(NPM_PACK_OUTPUT_DIR)
	cd editors/vscode && npm_config_cache=$(NPM_PACK_CACHE) npm run check && npm test && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >$(VSCODE_PACK_REPORT) && node test/pack_dry_run_test.js $(VSCODE_PACK_REPORT)

tree-sitter-package-check:
	mkdir -p $(NPM_PACK_CACHE) $(NPM_PACK_OUTPUT_DIR)
	cd tree-sitter-terlan && npm_config_cache=$(NPM_PACK_CACHE) npm run check && npm_config_cache=$(NPM_PACK_CACHE) npm run --silent pack:dry-run >$(TREE_SITTER_PACK_REPORT) && node test/pack_dry_run_test.js $(TREE_SITTER_PACK_REPORT)

$(TREE_SITTER_DEPENDENCY_STAMP): tree-sitter-terlan/package.json tree-sitter-terlan/package-lock.json
	mkdir -p $(NPM_PACK_CACHE)
	npm_config_cache=$(NPM_PACK_CACHE) npm ci --prefix tree-sitter-terlan --no-audit --no-fund
	touch $@

tree-sitter-cli-check: $(TREE_SITTER_DEPENDENCY_STAMP)
	mkdir -p $(TREE_SITTER_CLI_HOME) $(TREE_SITTER_CLI_CACHE)
	cd tree-sitter-terlan && HOME=$(TREE_SITTER_CLI_HOME) XDG_CACHE_HOME=$(TREE_SITTER_CLI_CACHE) npm run check:cli

neovim-editor-check:
	node editors/neovim/test/package_smoke_test.js

emacs-editor-check:
	node editors/emacs/test/package_smoke_test.js

intellij-editor-check:
	node editors/intellij/test/package_smoke_test.js
	cd editors/intellij && ./gradlew --no-daemon clean compileKotlin buildPlugin verifyPluginProjectConfiguration

shared-editor-icon-check:
	node editors/shared/test/icon_smoke_test.js

shared-editor-contract-check:
	node editors/shared/test/editor_contract_test.js

editor-debugger-surface-check:
	node editors/shared/test/debugger_surface_test.js
