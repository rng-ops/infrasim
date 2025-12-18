# Web Architecture (InfraSim)

This document explains—end to end—how InfraSim’s Web UI is built, served, and wired into the Rust web server on port 8080.

It is grounded in the actual repo implementation in:

- Rust web server: `infrasim/crates/web/src/main.rs`, `infrasim/crates/web/src/server.rs`, `infrasim/crates/web/src/static_files.rs`
- Console SPA: `infrasim/ui/apps/console/vite.config.ts`, `infrasim/ui/apps/console/src/main.tsx`, `infrasim/ui/apps/console/src/App.tsx`, `infrasim/ui/apps/console/index.html`
- API client used by the SPA: `infrasim/ui/packages/api-client/src/index.ts`

---

## 1) High-level system overview

At runtime, there are typically **three roles** involved:

1. **Browser**
   - Loads the console SPA under the `/ui/` path.
   - Calls JSON API endpoints under `/api/...`.
   - Optionally connects to a VNC websocket proxy under `/websockify/:vm_id`.

2. **Rust Web Server (Axum) – `infrasim-web`**
   - Listens on `INFRASIM_WEB_ADDR` (default: `127.0.0.1:8080`).
   - Serves:
     - `/api/...` JSON endpoints
     - `/ui/...` static SPA assets and SPA fallback
     - a small embedded set of noVNC-ish assets under `/app/*`, `/core/*`, `/vendor/*`
     - websocket proxy under `/websockify/:vm_id`

3. **InfraSim Daemon (gRPC)**
   - The web server talks to the daemon via gRPC at `INFRASIM_DAEMON_ADDR` (default: `http://127.0.0.1:50051`).
   - The web server is effectively a **REST/JSON façade** plus UI host.

### Request-flow map

Typical flows look like:

- Load UI shell
  - `GET http://127.0.0.1:8080/ui/` → Rust serves `index.html` from `INFRASIM_WEB_STATIC_DIR` (if configured).
  - The HTML loads JS/CSS assets (hashed in prod build).

- Call API
  - Browser → `GET/POST http://127.0.0.1:8080/api/...` → Axum handler.
  - Handler → gRPC daemon calls via `tonic` client → returns JSON to browser.

- Open VNC console
  - Browser → `GET ws://127.0.0.1:8080/websockify/:vm_id` → Axum upgrades and proxies.

---

## 2) Rust web server structure

### Entry point

The binary is started from `infrasim/crates/web/src/main.rs` and hands off to:

- `infrasim_web::server::serve(web_addr, cfg).await`

`cfg` includes:

- `daemon_addr` (gRPC upstream)
- web UI auth policy (token / JWT / dev random / none)

### Core server type

The actual server implementation lives in `infrasim/crates/web/src/server.rs`.

- `WebServer` owns an `Arc<WebServerState>`.
- `WebServerState` holds:
  - `daemon: DaemonProxy` (gRPC client wrapper)
  - `ui_static: UiStatic` (optional disk-backed SPA directory)
  - `static_files: StaticFiles` (embedded/placeholder noVNC files)
  - `tokens` (dev token storage)
  - `db` (local persistence via `infrasim_common::Database`)

### Environment variables

#### Binding / upstream

- `INFRASIM_WEB_ADDR`
  - Address the web server binds to.
  - Default: `127.0.0.1:8080`.

- `INFRASIM_DAEMON_ADDR`
  - gRPC endpoint the web server uses.
  - Default: `http://127.0.0.1:50051`.

#### UI static directory (production SPA)

- `INFRASIM_WEB_STATIC_DIR`
  - If set: enables serving the production-built SPA from disk.
  - The server reads it in `UiStatic::from_env()` in `server.rs`.
  - Expected to point at the Vite build output directory (see §4).

#### Auth

Auth is enforced by `auth_middleware` in `server.rs`.

- `INFRASIM_WEB_AUTH_TOKEN`
  - In static-token mode, the browser must send `Authorization: Bearer <token>`.

- JWT mode (implementation-supported; env wiring is in `main.rs`):
  - `INFRASIM_AUTH_MODE=jwt` and supporting issuer/audience/JWKS config.

- `INFRASIM_WEB_CONTROL_ENABLED=1`
  - Enables local admin control endpoints.

- `INFRASIM_WEB_ADMIN_TOKEN`
  - If set, admin endpoints require `x-infrasim-admin-token: ...`.

- `INFRASIM_DAEMON_PIDFILE`
  - Used by admin endpoints to signal the daemon for restart/stop.

### Router layout (Axum)

The Axum router is assembled in `WebServer::router()` (`server.rs`). It defines:

- `/api/...` routes for JSON operations
- VNC websocket proxy: `/websockify/:vm_id`
- Embedded static assets:
  - `/app/*path`
  - `/core/*path`
  - `/vendor/*path`
- Console UI routes:
  - `/ui`
  - `/ui/`
  - `/ui/*path`

There is also `.fallback(not_found_handler)` to handle unknown paths.

---

## 3) Rust auth model and what the browser must do

### The middleware gate

`auth_middleware` runs for essentially everything except a small allowlist:

- Unauthenticated allowed paths include:
  - `/` and embedded static assets (`/app/*`, `/core/*`, `/vendor/*`)
  - VNC websocket (`/websockify/...`)
  - `/api/health`

Everything else (including `/ui/...` and most `/api/...` endpoints) is protected unless auth is explicitly disabled.

### Token mode

In token mode, the client must attach:

- `Authorization: Bearer <token>`

The SPA does this automatically via `createApiClient()` (see §5). You still need to **set the initial token** into the UI’s store (mechanism depends on your UI login flow).

### JWT mode

In JWT mode, `Authorization: Bearer <jwt>` is validated server-side:

- signature verified against a local JWKS
- audience enforced
- issuer allowlisted

(Implementation is in `verify_jwt_with_local_jwks()` in `server.rs`.)

---

## 4) UI hosting: `/ui/` and why it’s special

InfraSim’s console is a **single-page application** that is intended to be mounted under `/ui/` (not `/`).

That “subpath hosting” has two crucial consequences:

1. **All production asset URLs must be prefixed with `/ui/`**
2. **Client-side routing must treat `/ui` as the basename**

### Production assets (Vite `base`)

The Vite config in `infrasim/ui/apps/console/vite.config.ts` sets:

- `base: "/ui/"`

This ensures that when you run `vite build`, generated `index.html` references assets like:

- `/ui/assets/index-<hash>.js`
- `/ui/assets/index-<hash>.css`

rather than `/assets/...`.

### Client-side router basename

In `infrasim/ui/apps/console/src/main.tsx`, the SPA is mounted with:

- `BrowserRouter basename="/ui"`

This means:

- a route declared as `/workspaces` is actually reachable at `/ui/workspaces`
- deep links like `/ui/workspaces/123/appliances` must serve **the same** `index.html` and let React Router take it from there

### SPA fallback on the server

The server’s `/ui/*path` handler must do two jobs:

- Serve real static files when they exist (`/ui/assets/...`, `/ui/favicon...`, etc.)
- If a file doesn’t exist (a client-side route), serve `/ui/index.html` as a fallback

`server.rs` explicitly notes this as an intended behavior (search for the comment `// SPA fallback: unknown routes map to index.html`).

---

## 5) How the SPA talks to the server (`/api/...`)

### API client package

The console app uses the internal package `@infrasim/api-client` (source: `infrasim/ui/packages/api-client/src/index.ts`).

Key properties:

- Uses `fetch()` and Zod to parse/validate responses.
- Uses TanStack React Query hooks for caching and refresh.

### Base URL and same-origin

The API client is created with:

- `baseUrl: ""` (same origin)

in `infrasim/ui/apps/console/src/api-context.tsx`.

That yields URLs like:

- `/api/daemon/status`
- `/api/appliances`

so:

- In production: browser calls the **same** `127.0.0.1:8080` origin.
- In development: Vite dev server can either proxy these (recommended) or you can set `baseUrl` to `http://127.0.0.1:8080`.

### Authorization header coupling

`createApiClient()` attaches:

- `Authorization: Bearer <token>` if `getToken()` returns one.

The UI’s `ApiProvider` wires `getToken` to the store (`state.auth.token`).

### API shape (selected endpoints)

The Axum router in `server.rs` defines (among others):

- Health / daemon
  - `GET /api/health`
  - `GET /api/daemon`
  - `GET /api/daemon/status`

- Inventory
  - `GET /api/vms`
  - `GET /api/vms/:vm_id`
  - `GET /api/volumes`
  - `GET /api/snapshots?vm_id=...`
  - `GET /api/networks`
  - `GET /api/images`

- Appliances
  - `GET /api/appliances/templates`
  - `GET /api/appliances`
  - `POST /api/appliances`
  - `GET /api/appliances/:appliance_id`
  - `POST /api/appliances/:appliance_id/boot`
  - `POST /api/appliances/:appliance_id/stop`
  - `POST /api/appliances/:appliance_id/snapshot`
  - `POST /api/appliances/:appliance_id/archive`
  - `GET /api/appliances/:appliance_id/export`
  - `POST /api/appliances/import`

- Terraform
  - `POST /api/terraform/generate`
  - `POST /api/terraform/audit`

- Provenance
  - `POST /api/provenance/attest`
  - `POST /api/provenance/evidence`

- Local admin control (optional)
  - `GET /api/admin/status`
  - `POST /api/admin/restart-web`
  - `POST /api/admin/restart-daemon`
  - `POST /api/admin/stop-daemon`

---

## 6) Development workflow (Vite) vs production workflow (Axum)

### Dev: Vite dev server as the UI host

In development you typically run:

- Rust web server on `:8080` for API + daemon coupling
- Vite dev server for fast HMR

The Vite config (`ui/apps/console/vite.config.ts`) is set up to proxy:

- `/api` → `http://127.0.0.1:8080`

So during dev, your browser can be on `http://127.0.0.1:4173/ui/...` and still call `/api/...` without CORS/auth headaches.

Important detail: because the app is intended for `/ui/`, the dev server should serve it with the same base path expectations. With Vite’s `base` and React Router `basename`, deep links should work as long as you visit the correct path prefix.

### Prod: Axum hosts the built SPA

In production, you build the SPA and point the Rust server to the output directory:

1. Build the SPA:
   - `pnpm -C infrasim/ui/apps/console build`
   - Output is typically `infrasim/ui/apps/console/dist/`

2. Run the web server with:
   - `INFRASIM_WEB_STATIC_DIR=/absolute/path/to/infrasim/ui/apps/console/dist`

Then:

- `GET /ui/` serves the built `index.html`
- `GET /ui/assets/...` serves hashed assets
- `GET /ui/workspaces/...` SPA fallback to `index.html`

---

## 7) DOM entry + how the SPA boots

In development, `infrasim/ui/apps/console/index.html` includes:

- `<div id="root"></div>`
- `<script type="module" src="/src/main.tsx"></script>`

Vite transforms this for production so that `index.html` loads the built asset graph. In either mode:

- `src/main.tsx` is the SPA bootstrap.
- It mounts the React tree into `#root`.

Basic boot sequence (conceptual):

1. Browser loads `GET /ui/`.
2. Browser loads JS/CSS assets referenced by `index.html`.
3. React mounts into the DOM.
4. `BrowserRouter` takes over navigation.
5. UI makes `fetch('/api/...')` calls through `@infrasim/api-client`.

---

## 8) Operational runbook (local)

### Run web server on 8080 (dev token)

Typical environment:

- `INFRASIM_WEB_ADDR=127.0.0.1:8080`
- `INFRASIM_DAEMON_ADDR=http://127.0.0.1:50051`

If auth defaults to `DevRandom`, the server prints a token on startup (`INFRASIM_WEB_AUTH_TOKEN (dev): ...`). The UI must use that token as `Authorization: Bearer ...`.

### Serve the built UI

Build + run:

- `pnpm -C infrasim/ui/apps/console build`
- `INFRASIM_WEB_STATIC_DIR=/path/to/infrasim/ui/apps/console/dist cargo run -p infrasim-web`

Then open:

- `http://127.0.0.1:8080/ui/`

### Verify quickly

- `curl http://127.0.0.1:8080/api/health`
- `curl http://127.0.0.1:8080/api/daemon/status` (requires daemon running and auth, depending on mode)

---

## 9) Appendix: noVNC static file stubs

The server also includes a minimal static file mechanism for embedded assets in `infrasim/crates/web/src/static_files.rs`.

- Route handlers map `/app/*`, `/core/*`, `/vendor/*` into `StaticFiles::serve(path)`.
- The current implementation is explicitly a stub (commented as minimal embedded JS).

This is **separate** from the console UI under `/ui/`.
