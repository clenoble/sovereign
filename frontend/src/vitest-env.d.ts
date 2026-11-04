// The test environment vitest builds from `vitest-setup.ts`, for the type
// checker (v0.0.12, F-a11y, F3, `doc/plans/V0012-frontend-a11y.md`). The setup
// file sits outside `src/`, so `svelte-check` never read it: neither the
// jest-dom matchers it adds to `expect` (`toBeInTheDocument`, `toBeEnabled`, ...)
// nor the `__tauriHandlers` global it declares were known to the checker,
// though vitest has them at run time. This reference adds the file to what the
// checker reads, so both come from the one place that defines them.
/// <reference path="../vitest-setup.ts" />
