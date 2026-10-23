# JavaScript browser integration tests

These tests start with Terlan source, run `terlc build --target js.browser`, read
the resulting build manifest, and load the emitted application in Playwright.
The compiler executable, input source, and emitted JS are hashed in
`target/js-browser/compilation.json`.

```sh
cargo build --locked -p terlan --bin terlc
npm --prefix tests/js ci --ignore-scripts
(cd tests/js && npx playwright install --with-deps chromium firefox webkit)
npm --prefix tests/js test
```

`TERLC` selects an already-built compiler. `TERLAN_JS_MODULES` accepts a
comma-separated module list for focused development runs. The pre-commit hook
clears that selection and runs the complete matrix for every nonempty commit.

The suite covers all modules under `std/js` and fails when a module has no test
inventory entry. Generated binding consumers exercise the compiler and host-call
contract: original JavaScript property names, receiver identity, argument order,
optional and rest arguments, nullability, and result forwarding. Controlled host
receivers isolate those contracts. Separate browser behavior cases use actual DOM
objects and JavaScript collections and exercise the compiled bridge tests.

Type-only modules are compiled and loaded, with an explicit annotation that they
have no runtime API. Neither these cases nor generated signature helpers count
as evidence that every browser API has been exercised. Generated helper files
contain typed calls, rather than constant passing test bodies.

Chromium, Firefox, and WebKit run without retries. Failures retain screenshots
and traces under `target/js-browser/test-results`; the machine-readable results
are in `target/js-browser/results.json`. The hook compares the discovered and
executed cases and rejects missing browsers, skips, expected failures, and retries.
