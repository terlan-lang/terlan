# Terlan editor integration validation targets.
#
# This file is included by the repository root Makefile so editor checks stay
# owned by the editor package layout while remaining available as root targets.

NPM_PACK_CACHE ?= /tmp/terlan-npm-cache

.PHONY: editor-help editor-check vscode-extension-check tree-sitter-package-check tree-sitter-cli-check neovim-editor-check emacs-editor-check intellij-editor-check shared-editor-icon-check shared-editor-contract-check

editor-help:
	@echo "  make editor-check - verify editor package contracts"
	@echo "  make vscode-extension-check - run VS Code extension syntax and smoke tests"
	@echo "  make tree-sitter-package-check - run Tree-sitter package metadata smoke"
	@echo "  make tree-sitter-cli-check - run Tree-sitter generate/test with local package deps"
	@echo "  make neovim-editor-check - run Neovim editor package smoke"
	@echo "  make emacs-editor-check - run Emacs editor package smoke"
	@echo "  make intellij-editor-check - run IntelliJ-family editor package smoke"
	@echo "  make shared-editor-icon-check - run shared editor icon smoke"
	@echo "  make shared-editor-contract-check - run cross-editor suffix and LSP contract smoke"

editor-check: vscode-extension-check tree-sitter-package-check neovim-editor-check emacs-editor-check intellij-editor-check shared-editor-icon-check shared-editor-contract-check

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
	cd tree-sitter-terlan && npm run check:cli

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
