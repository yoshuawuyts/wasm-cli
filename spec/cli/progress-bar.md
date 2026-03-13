# Progress Bar

The install command displays a phased progress UI for packages being installed.
The display transitions through distinct phases: syncing, planning, installing,
and done. Each phase uses a spinner or per-package progress bars to communicate
what the tool is doing.

## Spinner

r[cli.progress-bar.spinner-chars]
The phase spinner MUST use the Braille tick characters `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`.

r[cli.progress-bar.spinner-interval]
The spinner MUST tick at a steady interval of approximately 80ms.

## Phase Labels

r[cli.progress-bar.phase-syncing]
During registry sync the spinner MUST display `Syncing registry`.

r[cli.progress-bar.phase-planning]
During dependency resolution the spinner MUST display `Planning`.

r[cli.progress-bar.phase-installing]
During concurrent package downloads the spinner MUST display `Installing`.

r[cli.progress-bar.phase-done]
When all packages are installed the final line MUST read
`✓ Installed N packages in X.Xs` where N is the package count and X.X is
the elapsed time in seconds with one decimal place.

## Package Name Display

r[cli.progress-bar.namespace-name]
The package MUST be displayed in `namespace:name` form, not the full OCI URL.

r[cli.progress-bar.version-display]
The version MUST be displayed as a space-separated value (e.g. `wasi:http 0.2.3`),
not with an `@` separator.

r[cli.progress-bar.flat-rows]
Packages MUST be displayed as a flat, column-aligned list without tree glyphs.

r[cli.progress-bar.name-color-downloading]
Package names MUST be unstyled (default terminal color) while a download is in
progress.

r[cli.progress-bar.name-color-complete]
Package names MUST be displayed in green once the download is complete.

## Plan Display

r[cli.progress-bar.plan-timing]
The resolved plan MUST be displayed after the planning phase completes and
before the installing phase begins.

## Per-Package Progress

r[cli.progress-bar.bar-yellow]
The progress bar MUST be displayed in yellow.

r[cli.progress-bar.checkmark-complete]
When a package download completes its row MUST display a green `✓` marker
instead of the progress bar.

r[cli.progress-bar.size-grey]
The download size MUST be displayed in grey.

r[cli.progress-bar.eta-grey]
The estimated time remaining MUST be displayed in grey.

r[cli.progress-bar.aggregate-layers]
The progress MUST show the overall progress for the entire package, not
individual layers.
