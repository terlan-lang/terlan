# Mobile Runtime Planning

The `mobile` module owns Terlan mobile delivery planning, shell metadata, and
Angular bridge support.

The compiler remains responsible for parsing, typing, CoreIR, and target
profile validation. Mobile code consumes those public contracts and emits
mobile-specific planning artifacts, shell layouts, bridge metadata, widget
metadata, route metadata, and native capability metadata.

Implementation files for mobile delivery must stay in this module. Compiler
modules may keep explicit integration hooks when they adapt mobile validation
into compiler diagnostics, but mobile-owned implementation must not live under
`crates/terlan/src/compiler`.
