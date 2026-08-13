# AngularTS Root Makefile Hooks

Insert these recipe lines into the existing AngularTS root `Makefile` targets
after materializing `integrations/terlan`. Keep any existing prerequisites such
as `generated-check: types` or `test-integrations: ensure-deps`.

```make
generated-check:
	@$(MAKE) -C integrations/terlan generate-check
	@$(MAKE) -C integrations/wasm/terlan generate-check

test-integrations:
	@$(MAKE) -C integrations/terlan check
	@$(MAKE) -C integrations/wasm/terlan check
```

These hooks keep Terlan generation freshness and the runnable todo harness in
the same root gates as the other AngularTS integrations.
