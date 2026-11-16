# ---- Compiler/tooling bootstrap targets not already guarded by a conditional above ----

# GNU Make 4.3 treats its global-serialization special target as process-wide.
# Keep this target's required ordering explicit without disabling safe
# parallelism elsewhere in the build graph.
terlan-quality-bootstrap: rust-test-suite
	$(MAKE) --no-print-directory --jobs=1 terlan-quality-tools-bootstrap


# Aggregates every typed-validator bootstrap producer behind the self-validation boundary.
terlan-self-validation-bootstrap: terlan-compiler-bootstrap terlan-artifact-measurement-bootstrap terlan-make-recipe-bootstrap terlan-semantic-kernel-bootstrap terlan-ebnf-validator-bootstrap terlan-shared-helper-bootstrap terlan-external-package-matrix-bootstrap terlan-tvm-package-consumer-bootstrap terlan-tvm-platform-matrix-bootstrap terlan-rust-quality-bootstrap terlan-docs-static-release-parity-bootstrap terlan-release-promotion-bootstrap terlan-web-manifest-preflight-bootstrap terlan-self-validation-checkout-bootstrap terlan-stdlib-validation-bootstrap terlan-repository-validation-bootstrap terlan-proof-release-bootstrap terlan-release-control-bootstrap

# Proof consumers need this image, not the entire validator fleet. The aggregate
# still shares this producer with focused consumers in the same Make invocation.


