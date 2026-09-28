# Dependency upgrade deferrals

The following Dependabot upgrades were reviewed for 0.0.9 and deferred. The
duplicate-family ceiling remains 34. Exact-version ignores allow later releases
to be reviewed normally; they do not disable security auditing.

| PR | Upgrade | Reason and condition for reconsideration |
| --- | --- | --- |
| [#26](https://github.com/terlan-lang/terlan/pull/26) | instant-acme 0.7.2 → 0.8.5 | The proposed lockfile adds duplicate Base64, PEM, rcgen, untrusted, and yasna families. Both upstream crypto-provider features enable rcgen 0.14, while Terlan uses rcgen 0.13. A coordinated certificate-generation and ACME API migration, with dependency consolidation, is required before adoption. |
| [#29](https://github.com/terlan-lang/terlan/pull/29) | html5ever 0.39.0 → 0.40.1 | Ammonia still requires html5ever 0.39. The proposed lockfile adds nine duplicate families: html5ever, markup5ever, phf, phf_codegen, phf_generator, phf_shared, string_cache, string_cache_codegen, and web_atoms. Reconsider when the sanitizer and parser dependencies can migrate together. |

These deferrals do not change existing runtime behavior or relax quality gates.
