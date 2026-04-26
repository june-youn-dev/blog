# blog

This repository currently consists of two packages:

- [`apps/api`](./apps/api): Rust Cloudflare Worker API backed by D1
- [`apps/site`](./apps/site): Eleventy-based public site and lightweight browser admin console

## Current focus

The codebase already supports:

- public post listing and detail retrieval
- authenticated administrative post management
- Firebase-backed single-admin browser sign-in
- static site generation from API content

Still intentionally out of scope:

- multi-user administration
- role-based authorization
- comments

## Local start

Start the API first:

```sh
cd blog/apps/api
nvm use
pnpm install
pnpm rebuild workerd esbuild
pnpm run migrate:local
pnpm run worker:dev
```

Then build and serve the site:

```sh
cd blog/apps/site
nvm use
pnpm install
BLOG_API_URL=http://localhost:8787 pnpm run build
pnpm run dev
```

For package-specific setup and commands, read:

- [`apps/api/README.md`](./apps/api/README.md)
- [`apps/site/README.md`](./apps/site/README.md)

## Standard verification

Run the full repository verification path with:

```sh
cd blog
bash scripts/check-all.sh
```

Repository-wide engineering rules live in [`ENGINEERING_GUARDRAILS.md`](./ENGINEERING_GUARDRAILS.md).
