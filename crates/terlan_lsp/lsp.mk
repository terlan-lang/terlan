# Terlan language-server validation targets.
#
# This file is included by the repository root Makefile so LSP checks stay
# owned by the language-server crate while remaining available as release gates.

.PHONY: lsp-help lsp-check lsp-protocol-check

lsp-help:
	@echo "  make lsp-check - run language-server protocol smoke tests"
	@echo "  make lsp-protocol-check - run focused LSP initialize/diagnostic/lifecycle/template/symbol/definition tests"

lsp-check: lsp-protocol-check

lsp-protocol-check:
	$(CARGO) test -p terlan_lsp lib_test::smoke_initialize_and_shutdown -- --exact
	$(CARGO) test -p terlan_lsp lib_test::did_open_reports_parse_diagnostic -- --exact
	$(CARGO) test -p terlan_lsp lib_test::did_open_reports_type_diagnostic -- --exact
	$(CARGO) test -p terlan_lsp lib_test::did_change_reports_parse_diagnostic -- --exact
	$(CARGO) test -p terlan_lsp lib_test::did_close_is_accepted -- --exact
	$(CARGO) test -p terlan_lsp lib_test::did_open_template_document_publishes_clear_diagnostics -- --exact
	$(CARGO) test -p terlan_lsp lib_test::did_open_invalid_template_document_publishes_template_diagnostic -- --exact
	$(CARGO) test -p terlan_lsp lib_test::document_symbol_request_returns_nested_symbols -- --exact
	$(CARGO) test -p terlan_lsp lib_test::document_symbol_request_returns_empty_for_template_documents -- --exact
	$(CARGO) test -p terlan_lsp lib_test::definition_request_returns_same_document_location -- --exact
	$(CARGO) test -p terlan_lsp lib_test::definition_request_returns_empty_for_imported_reference -- --exact
	$(CARGO) test -p terlan_lsp lib_test::definition_request_returns_empty_for_template_documents -- --exact
