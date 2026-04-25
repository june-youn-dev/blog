# @blog/api

`@blog/api` is the operational backend for the blog. It runs on Cloudflare Workers, is written in Rust, and stores data in Cloudflare D1.

## What this package does

- serves the public post API
- serves the authenticated administrative API
- verifies Firebase ID tokens for a single administrator
- issues and validates the admin session cookie
- exports OpenAPI and TypeScript bindings

## Local setup

```sh
cd blog/apps/api
nvm use
pnpm install
pnpm rebuild workerd esbuild
rustup toolchain install nightly --component rustfmt --profile minimal
pnpm run migrate:local
```

Create `.dev.vars`:

```sh
cat > .dev.vars <<'EOF'
API_KEY=dev-secret-change-me
ADMIN_SESSION_SECRET=dev-session-secret-change-me
FIREBASE_PROJECT_ID=your-firebase-project-id
ADMIN_FIREBASE_UID=your-admin-firebase-uid
ADMIN_ORIGIN=http://localhost:8080
EOF
```

Minimum meanings:

- `API_KEY`: bearer token for administrative API access
- `ADMIN_SESSION_SECRET`: signing secret for the admin session cookie
- `FIREBASE_PROJECT_ID`: Firebase project used to validate browser sign-in tokens
- `ADMIN_FIREBASE_UID`: the single allowed administrator identity
- `ADMIN_ORIGIN`: allowed browser origin for credentialed administrative requests

## Run locally

```sh
pnpm run worker:dev
```

Default local address:

```text
http://localhost:8787
```

Simple check:

```sh
curl http://localhost:8787/
```

## API documentation

The authoritative API documentation is generated from code.

- OpenAPI JSON: `http://localhost:8787/openapi.json`
- Swagger UI: `http://localhost:8787/docs`

The API README is intentionally brief. Endpoint details belong to OpenAPI and the Rust source.

## TypeScript bindings

TypeScript bindings for the site live in [../site/src/api-types](../site/src/api-types).

Generate them with:

```sh
pnpm run bindings:generate
```

Check them with:

```sh
pnpm run bindings:check
```

## Common commands

```sh
pnpm run migrate:local
pnpm run sql:lint
pnpm run sql:fix
pnpm run rust:check
pnpm run rust:test
pnpm run rust:lint
pnpm run rust:fmt
pnpm run rust:fmt:check
pnpm run worker:dev
pnpm run rust:build
pnpm run bindings:generate
pnpm run bindings:check
```
