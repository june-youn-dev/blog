# @blog/api

`@blog/api` is the workspace for the blog backend. It contains the public Worker, the administrative Worker, and the shared Rust core crate, all backed by Cloudflare D1.

## What this package does

- builds the public read-only Worker
- builds the authenticated administrative Worker
- stores shared DTO, validation, and database logic in a reusable core crate
- exports TypeScript bindings for the site

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

## Layout

- `public/`: public read-only Worker
- `admin/`: authenticated administrative Worker
- `core/`: shared DTO, validation, and D1 logic
- workspace root (`apps/api`): the only supported command entrypoint

## Run locally

```sh
pnpm run dev
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

The public Worker does not expose OpenAPI. The administrative Worker remains a separate deployment surface and is intentionally documented in code rather than in this README.

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
pnpm run check
pnpm run migrate:local
pnpm run migrate:remote
pnpm run dev
pnpm run dev:admin
pnpm run build
pnpm run build:admin
pnpm run deploy:check
pnpm run deploy:check:admin
pnpm run deploy
pnpm run deploy:admin
pnpm run sql:lint
pnpm run sql:fix
pnpm run rust:check
pnpm run rust:test
pnpm run rust:lint
pnpm run rust:fmt
pnpm run rust:fmt:check
pnpm run bindings:generate
pnpm run bindings:check
```
