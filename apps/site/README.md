# @blog/site

`@blog/site` is the static frontend for the blog. It builds public pages with Eleventy and also provides a lightweight browser-based administrative console at `/admin/`.

## What this package does

- fetches public posts from the API
- renders AsciiDoc into static HTML
- serves the public site
- serves the browser admin console

## Local setup

```sh
cd blog/apps/site
nvm use
pnpm install
```

Build-time environment for the admin console:

```sh
BLOG_API_URL=http://localhost:8787
FIREBASE_PROJECT_ID=your-firebase-project-id
FIREBASE_WEB_API_KEY=your-firebase-web-api-key
FIREBASE_APP_ID=your-firebase-web-app-id
```

Optional:

- `FIREBASE_AUTH_DOMAIN`

If omitted, the site derives `<FIREBASE_PROJECT_ID>.firebaseapp.com`.

## Common commands

Fetch public posts from the API:

```sh
BLOG_API_URL=http://localhost:8787 pnpm run fetch
```

Check the fetch pipeline against the local fixture server:

```sh
pnpm run fetch:check
```

Build browser modules:

```sh
pnpm run ts:build
```

Type-check Node/browser TypeScript:

```sh
pnpm run ts:check
```

Build the site:

```sh
BLOG_API_URL=http://localhost:8787 pnpm run build
```

Run the development server:

```sh
pnpm run dev
```

Clean generated output:

```sh
pnpm run clean
```

## Local browser check

1. Start the API in `blog/apps/api` with `pnpm run worker:dev`.
2. Build the site once with the required environment variables.
3. Run `pnpm run dev`.
4. Open `http://localhost:8080/`.
5. Open `http://localhost:8080/admin/`.

This README is intentionally brief. Rendering details, security filters, and internal build flow belong in the source files rather than here.
