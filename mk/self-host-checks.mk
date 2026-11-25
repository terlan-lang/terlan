# ---- Compiler self-hosting checks ----

# Runs the typed stdlib capability test surface (file/dir/hash/json/regex/string) through the debug compiler.
terlan-self-validation-capabilities-check:
	target/debug/terlc test \
		std/system \
		std/io/FileTest.terl \
		std/io/DirectoryTest.terl \
		std/crypto/HashTest.terl \
		std/data/JsonTest.terl \
		std/regex/RegexTest.terl
	target/debug/terlc test std/core/StringTest.terl \
		--name contains_accepts_present_pattern \
		--name starts_with_accepts_prefix \
		--name ends_with_accepts_suffix \
		--name lowercase_converts_ascii_text \
		--name direct_length_counts_unicode_scalars \
		--name direct_byte_size_counts_utf8_bytes \
		--name trim_removes_surrounding_whitespace \
		--name trim_start_removes_leading_whitespace \
		--name trim_end_removes_trailing_whitespace \
		--name compare_orders_earlier_text_first \
		--name compare_orders_later_text_after \
		--name compare_accepts_equal_text


# One public closeout owns the typed capability surface, generated std
# contracts, thin Make dispatch, browser-manifest
# consumers, documentation validation, and release-promotion orchestration.
# Every prerequisite reuses the compiler and sealed images from the shared
# bootstrap boundary.
terlan-self-validation-check: \
	terlan-self-validation-clean-checkout-check \
	terlan-self-validation-capabilities-check \
	terlan-make-recipe-thinness-check \
	stdlib-summary-drift-check \
	stdlib-native-artifacts-check \
	stdlib-rust-backed-manifest-check \
	stdlib-release-manifest-check \
	docs-light-check \
	browser-package-preflight \
	web-profile-preflight \
	release-promotion-pipeline-check
	@echo "[terlan-self-validation] direct-AOT validation passed without unapproved host scripts"


# Runs the sealed self-validation-checkout image against the repository root.
terlan-self-validation-clean-checkout-check: terlan-self-validation-checkout-bootstrap
	$(TERLAN_SELF_VALIDATION_CHECKOUT) "$(CURDIR)"


