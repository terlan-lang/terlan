# Vendored `angular.css`

This directory contains the compiled CSS consumed by terlan.io. It was copied
from the explicitly selected dirty local checkout at
`/home/anatoly/Applications/ng/angular.css`.

- Source revision: `433b074a6a8c54700ae8b147514e1296d62cd80c`
- Source worktree: dirty by design
- Package version: `0.0.1`
- CSS compiler: Tailwind CSS `4.3.0`
- Artifact SHA-256:
  `a2852ff5aa1eb008aa56cbe14528ab11773a73300811c401052cdc0609fabfd2`
- License: MIT; see `LICENSE`

Tailwind resolves the component source's `@apply` rules. The resulting
artifact is ordinary CSS. Consumer theming uses the semantic custom properties
declared by `angular.css`, overridden in `assets/site.css`; GitHub Pages and
terlan.io therefore do not install or execute Tailwind.

Run `scripts/check-angular-css.sh` to verify the checked-in artifact. When a
local angular.css checkout is available, the same script also reports whether
its current build differs from this selected snapshot.
