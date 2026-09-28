# Dependency upgrade deferrals

The following Dependabot upgrades were reviewed for 0.0.9 and deferred. The
duplicate-family ceiling remains 34. Ignores cover the blocked release lines so
Dependabot cannot replace a rejected patch with an older patch from the same
incompatible line. Later release lines remain eligible for review. Security
auditing remains enabled.

| PR | Upgrade | Reason and condition for reconsideration |
| --- | --- | --- |
| [#16](https://github.com/terlan-lang/terlan/pull/16) | Base64 0.22 → 0.23 | Existing ACME, PEM, and HTTP clients retain Base64 0.22. The new line adds a 35th duplicate family. Reconsider after those consumers migrate or other duplicates are removed. |
| [#26](https://github.com/terlan-lang/terlan/pull/26) | instant-acme 0.7.2 → 0.8.5 | The proposed lockfile adds duplicate Base64, PEM, rcgen, untrusted, and yasna families. Both upstream crypto-provider features enable rcgen 0.14, while Terlan uses rcgen 0.13. A coordinated certificate-generation and ACME API migration, with dependency consolidation, is required before adoption. |
| [#29](https://github.com/terlan-lang/terlan/pull/29), [#35](https://github.com/terlan-lang/terlan/pull/35), [#36](https://github.com/terlan-lang/terlan/pull/36) | html5ever 0.39 → 0.40; Ammonia 4.1 → 4.2 | Updating the parser alone adds nine duplicate families. Updating Ammonia, html5ever, and cssparser together still leaves duplicate phf, phf_codegen, phf_generator, phf_shared, string_cache, and string_cache_codegen families: Comrak/Oxc retain phf 0.13 and LALRPOP retains string_cache 0.9, while web_atoms 0.3 brings the new generations. Reconsider with a coordinated parser, sanitizer, and transitive dependency migration that fits the unchanged budget. |

These deferrals do not change existing runtime behavior or relax quality gates.

ZIP [#37](https://github.com/terlan-lang/terlan/pull/37), SQL parser
[#38](https://github.com/terlan-lang/terlan/pull/38), and Rustix
[#39](https://github.com/terlan-lang/terlan/pull/39) are integrated. For
[#41](https://github.com/terlan-lang/terlan/pull/41), Tungstenite advances to 0.29;
0.30 is deferred because it adds duplicate rand and rand_core families through
Foundations' governor dependency, taking the graph from 34 to 36 families.

Oxc [#40](https://github.com/terlan-lang/terlan/pull/40) cannot be adopted as a
standalone oxc_ast update. Resolving all six
direct Oxc crates together still produces 39 duplicate families: compact_str
and four PHF families are added. Oxc non-patch updates are held for the 0.0.9
coordinated frontend migration; patch updates remain enabled and grouped.
Remove that hold after the frontend migration and dependency consolidation pass
the unchanged budget and JavaScript backend checks.

CSS parser [#42](https://github.com/terlan-lang/terlan/pull/42) is part of the
same coordinated HTML migration: 0.38 adds duplicate cssparser, phf,
phf_generator, phf_macros, and phf_shared families, raising the total to 39.
Ammonia 4.1 retains cssparser 0.37, and the current Comrak/Oxc stack retains
PHF 0.13. The 0.38 release line is held until that migration fits the budget.

Object [#43](https://github.com/terlan-lang/terlan/pull/43), HTTP
[#44](https://github.com/terlan-lang/terlan/pull/44), time
[#45](https://github.com/terlan-lang/terlan/pull/45), and Comrak
[#46](https://github.com/terlan-lang/terlan/pull/46) are integrated. Comrak's
accessible heading links put the fragment ID on the heading itself; the
metadata extractor now reads that ID and preserves unique heading fragments.
