# AOT Compilation Fixtures

These fixtures define equivalent small-command and multi-package workloads for
Terlan and Go compilation measurements. Both implementations compute and check
the same integer result. Benchmark setup copies these files into isolated
workspaces before changing package implementation bodies.

The fixtures intentionally avoid external dependencies so measurements cover
compiler startup, project analysis, native object generation, cache reuse, and
linking rather than package download time.
