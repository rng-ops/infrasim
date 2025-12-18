# @infrasim/api-client

Typed API client + TanStack Query hooks for InfraSim.

## Goals

- Centralize fetch + error handling
- Validate responses with Zod (fail fast when backend changes)
- Provide hooks for common views (status, vms, appliances, snapshots, terraform)

## Usage

```ts
import { createApiClient } from "@infrasim/api-client";

const client = createApiClient({
  baseUrl: "",
  getToken: () => sessionStorage.getItem("infrasim.token"),
  onUnauthorized: () => {
    // route to login, clear token, etc.
  },
});
```

## Error handling

All errors thrown by `request()` are `ApiError`:

- `status`: HTTP status
- `message`: backend `error` / `message` if present
- `details`: parsed JSON body when possible (or raw text)

When the API returns 401, `onUnauthorized()` is called.

## Security notes

- Prefer `sessionStorage` for tokens during development.
- Avoid logging tokens anywhere (console, telemetry, error reporting).
- Treat all server strings as untrusted; render via React escaping (never `dangerouslySetInnerHTML`).
- Keep `baseUrl` same-origin in production to reduce token exfil via misconfig.
