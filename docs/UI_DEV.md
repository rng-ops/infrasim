# UI Development

The InfraSim Console lives in `infrasim/ui/` as a pnpm workspace:

- `ui/apps/console` — Vite + React SPA
- `ui/packages/infrasim-ui` — shared design system (`@infrasim/ui`)
- `ui/packages/api-client` — typed-ish client + hooks (`@infrasim/api-client`)

## Quickstart

From `infrasim/`:

- Install deps: `make ui-install`
- Run dev server: `make ui-dev` (Vite on `http://127.0.0.1:4173`)
- Typecheck: `make ui-typecheck`
- Build: `make ui-build` (outputs `ui/apps/console/dist/`)

The dev server proxies `/api/*` to `http://127.0.0.1:8080` (see `ui/apps/console/vite.config.ts`).

## Module resolution (workspace packages)

The console imports workspace packages by name:

- `@infrasim/ui`
- `@infrasim/api-client`

In dev mode, pnpm links these packages into `ui/apps/console/node_modules`.
In production build mode, Vite resolves them using their package entrypoints.

Important:
- `ui/apps/console/index.html` is the Vite build entry module. Without it, `vite build` fails.

## TypeScript and declarations

The workspace runs strict typechecking (`pnpm -r typecheck`).

`@infrasim/ui` currently emits `.d.ts` via tsup.

`@infrasim/api-client` currently does **not** emit `.d.ts` in its build step due to a TypeScript project-graph quirk (TS6307) under the current `tsconfig.base.json` (`moduleResolution: Bundler`).
This does not affect running or building the console, and typechecking still runs at the workspace level.

If you want published/consumable `.d.ts` for `@infrasim/api-client`, we should either:
- switch its build-time tsconfig/moduleResolution to a NodeNext-only setup, or
- move to `tsup` dts bundling via an API Extractor workflow.

## Security notes

- Treat tokens as secrets: store in `sessionStorage` by default and never log them.
- The dev server proxy is for local dev; prefer same-origin deployment in production.
# UI Development

The InfraSim Console is a Vite/React SPA in `ui/apps/console`.

## Local dev

Prereqs:
- Node + pnpm
- Rust toolchain

Install UI deps:

`make ui-install`

Run the web backend:

`cd infrasim && INFRASIM_WEB_ADDR=127.0.0.1:8080 INFRASIM_DAEMON_ADDR=http://127.0.0.1:50051 cargo run -p infrasim-web --bin infrasim-web`

Run the UI dev server:

`make ui-dev`

The Vite dev server proxies `/api/*` to the backend (see `ui/apps/console/vite.config.ts`).

## Production build + Rust serving

Build the SPA:

`make ui-build`

This produces a `dist/` directory under the Vite app (typically `ui/apps/console/dist`).

Configure the Rust web server to serve those assets:

- Set `INFRASIM_WEB_STATIC_DIR` to the build directory.
- Visit `http://127.0.0.1:8080/ui`.

Example:

`INFRASIM_WEB_STATIC_DIR=ui/apps/console/dist cargo run -p infrasim-web --bin infrasim-web`

Routing behavior:
- `/ui/...` serves assets from the dist directory
- unknown `/ui/<route>` paths fall back to `/ui/index.html` (SPA fallback)

## State management

The console uses a small Vuex-like store in `ui/apps/console/src/store/store.ts`:

- `state` is immutable and JSON-serializable
- `commit(mutation)` is the only way to mutate state
- `actions` encapsulate higher-level flows (login/logout, toasts, renderer patches)

We also track page visibility (`document.visibilityState`) to support:
- pausing polling when the page is hidden
- reducing renderer work for WebGPU/canvas views
