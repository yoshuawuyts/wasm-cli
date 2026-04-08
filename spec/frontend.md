# Frontend

The frontend is a server-side rendered web application compiled as a
`wasm32-wasip2` component targeting `wasi:http`. It provides a public
browsing UI for the package registry.

## Server

r[frontend.server.wasi-http]
The frontend MUST be compiled as a `wasm32-wasip2` component targeting
`wasi:http`, served via `wstd-axum`.

r[frontend.server.health]
The `/health` endpoint MUST return `200 OK` with `{"status": "ok"}`.

## Pages

r[frontend.pages.home]
The front page (`/`) MUST display a list of recently updated components
and interfaces, fetched from the meta-registry API. Components and
interfaces MUST be shown in separate sections.

r[frontend.pages.package-detail]
The package detail page (`/<namespace>/<name>/<version>`) MUST display
basic information about a package including its name, description,
registry, repository, tags, dependencies, and timestamps.

r[frontend.pages.package-redirect]
Requests to `/<namespace>/<name>` (without a version) MUST redirect to
`/<namespace>/<name>/<latest-version>`.

r[frontend.pages.all]
The `/all` page MUST display a paginated list of all known packages.

r[frontend.pages.not-found]
Requests for unknown routes MUST return a `404 Not Found` response with
a user-friendly error page that links back to the home page.

## Routing

r[frontend.routing.package-path]
Components and interfaces MUST be addressable at
`/<namespace>/<name>/<version>`.

r[frontend.routing.reserved-namespaces]
A list of reserved namespaces (e.g. `login`, `all`, `health`) MUST be
maintained so that reserved paths are never interpreted as package
lookups.

## API Client

r[frontend.api.callback]
The frontend component MUST call back to the meta-registry API service
to fetch package data, using outgoing HTTP requests via `wstd::http::Client`.

r[frontend.api.base-url]
The API base URL MUST be configurable via a compile-time environment variable
(`API_BASE_URL`), falling back to `http://localhost:8081`.

## Rendering

r[frontend.rendering.html-crate]
All dynamic HTML content MUST be generated server-side using the `html`
crate's type-safe builder API.

r[frontend.rendering.static]
Pages SHOULD be almost entirely static — no client-side JavaScript is
required for core functionality.

## Styling

r[frontend.styling.accent-color]
The accent color MUST be `#512FEB` (R81 G47 B235).

r[frontend.styling.tailwind]
Tailwind CSS MUST be used as the CSS framework.

r[frontend.styling.responsive]
The layout MUST be responsive and mobile-first.

r[frontend.styling.light-theme]
The frontend MUST start with a light theme.

r[frontend.styling.dark-mode]
The frontend MUST support dark mode via `prefers-color-scheme: dark`,
automatically switching themes based on the user's system preferences.

## Caching

r[frontend.caching.static-pages]
HTML responses SHOULD include appropriate `Cache-Control` headers to
optimize for caching. Home page responses SHOULD use a short TTL
(e.g. 60 seconds), package detail pages a medium TTL (e.g. 300 seconds),
and error pages SHOULD NOT be cached.
