# Std JS Intl Namespace

This directory contains generated Terlan bindings for the JavaScript `Intl`
namespace. It provides target-specific collation, date/time formatting, and
number formatting surfaces derived from pinned TypeScript declarations.

## Responsibilities

- Model constructors, formatter objects, options, and resolved options.
- Preserve declaration names and optional fields through generation.
- Keep generated source, interfaces, summaries, and tests in sync.

## Core Model

Intl objects are JavaScript-native values and are available only on compatible
JS targets. Portable formatting APIs must not depend on these host objects.

## Integration Points

- `commands::bind` owns `.d.ts` parsing and Terlan source generation.
- target-profile validation prevents Intl imports on VM and Wasm targets.

## Testing Notes

- Adjacent `*Test.terl` files cover generated constructors and option records.
- Regenerate through the pinned binding pipeline; do not hand-edit generated
  modules.
