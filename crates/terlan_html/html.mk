# Terlan artifact-template validation targets.
#
# This file is included by the root Makefile. Target names remain public from
# the repository root, while template target validation stays owned by the
# `terlan_html` crate.

.PHONY: html-help artifact-template-check

html-help:
	@echo "  make artifact-template-check - run artifact-template and Terlan HTML validation checks"

artifact-template-check:
	$(CARGO) test -p terlan_html -- --nocapture
