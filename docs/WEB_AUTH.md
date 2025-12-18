# Web Auth (JWT)

Date: 2025-12-15

InfraSim Web UI (`infrasim-web`) supports JWT-based authentication for `/api/*`.

## Mode: Local Issuer (Self-Signed)

This mode validates JWTs against a local JWKS file on disk. It is intended for:
- local development
- air-gapped deployments
- "appliance" deployments where you control the IdP keys

### Configure

- `INFRASIM_WEB_ADDR` (default `127.0.0.1:8080`)
- `INFRASIM_DAEMON_ADDR` (default `http://127.0.0.1:50051`)

Auth settings:
- `INFRASIM_AUTH_MODE=jwt`
- `INFRASIM_AUTH_ALLOWED_ISSUERS` (comma-separated, required)
- `INFRASIM_AUTH_AUDIENCE` (required)
- `INFRASIM_AUTH_LOCAL_JWKS_PATH` (required): path to a JWKS JSON file

### Request

Send a bearer token:

- `Authorization: Bearer <JWT>`

The server enforces:
- `iss` is in `INFRASIM_AUTH_ALLOWED_ISSUERS`
- `aud` contains `INFRASIM_AUTH_AUDIENCE`
- token is correctly signed by a key in `INFRASIM_AUTH_LOCAL_JWKS_PATH`

## Notes

- Static assets and `/api/health` remain unauthenticated.
- This is the foundation for adding approved external issuers later (Okta / Keycloak) via OIDC discovery + JWKS fetching.
