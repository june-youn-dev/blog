import { createServer } from "node:http";
import { access, mkdtemp, readFile, rename, rm } from "node:fs/promises";
import { spawn } from "node:child_process";
import { join } from "node:path";
import { tmpdir } from "node:os";

export const fixturePosts = [
  {
    public_id: "9aa5b0c0-7d2b-4c75-90a6-4f96fef7d3f8",
    slug: "fixture-post-one",
    title: "Fixture Post One",
    summary: "The first fixture post.",
    body_adoc: "= Fixture Post One\n\nA fixture body.\n",
    published_at: "2026-04-26T00:00:00Z",
  },
  {
    public_id: "dd87a225-3ef1-4e1d-baf7-97cfd8227e2d",
    slug: "fixture-post-two",
    title: "Fixture Post Two",
    summary: null,
    body_adoc: "= Fixture Post Two\n\nAnother fixture body.\n",
    published_at: "2026-04-25T12:30:00Z",
  },
] as const;

export const rootDir = join(import.meta.dirname, "..");
export const postsDir = join(rootDir, "src", "posts");
export const permalinksDir = join(rootDir, "src", "permalinks");
export const siteOutputDir = join(rootDir, "_site");

export async function withFixtureApi<T>(callback: (apiUrl: string) => Promise<T>): Promise<T> {
  const server = createServer((request, response) => {
    if (!request.url) {
      response.writeHead(400).end();
      return;
    }

    const url = new URL(request.url, "http://127.0.0.1");
    if (url.pathname === "/posts") {
      response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(
        fixturePosts.map(({ public_id, slug, title, summary, published_at }) => ({
          public_id,
          slug,
          title,
          summary,
          published_at,
        })),
      ));
      return;
    }

    const detailMatch = url.pathname.match(/^\/posts\/by-id\/([^/]+)$/);
    if (detailMatch) {
      const post = fixturePosts.find((item) => item.public_id === decodeURIComponent(detailMatch[1]));
      if (!post) {
        response.writeHead(404).end(JSON.stringify({ error: "not found" }));
        return;
      }
      response.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(post));
      return;
    }

    response.writeHead(404).end(JSON.stringify({ error: "not found" }));
  });

  const address = await new Promise<{ port: number }>((resolve, reject) => {
    server.listen(0, "127.0.0.1", () => {
      const listening = server.address();
      if (!listening || typeof listening === "string") {
        reject(new Error("Failed to determine fixture server address."));
        return;
      }
      resolve({ port: listening.port });
    });
    server.once("error", reject);
  });

  try {
    return await callback(`http://127.0.0.1:${address.port}`);
  } finally {
    await new Promise<void>((resolve) => {
      server.close(() => resolve());
    });
  }
}

export async function withPreservedDirectories<T>(
  targets: readonly string[],
  callback: () => Promise<T>,
): Promise<T> {
  const backupRoot = await mkdtemp(join(tmpdir(), "blog-site-generated-"));
  const backups = new Map<string, string>();

  try {
    for (const target of targets) {
      const backup = join(backupRoot, backups.size.toString());
      try {
        await rename(target, backup);
        backups.set(target, backup);
      } catch (error) {
        if (!isMissingPathError(error)) {
          throw error;
        }
      }
    }

    return await callback();
  } finally {
    await Promise.all(targets.map(async (target) => {
      await rm(target, { recursive: true, force: true });
      const backup = backups.get(target);
      if (backup) {
        await rename(backup, target).catch(() => {});
      }
    }));
    await rm(backupRoot, { recursive: true, force: true });
  }
}

export async function runFetchPosts(apiUrl: string): Promise<void> {
  process.env.BLOG_API_URL = apiUrl;
  await import(new URL(`./fetch-posts.mts?run=${Date.now()}`, import.meta.url).href);
}

export async function runEleventyBuild(): Promise<void> {
  await runCommand("pnpm", ["exec", "eleventy"], rootDir);
}

export async function assertFixtureFetchOutput(): Promise<void> {
  await access(join(postsDir, "fixture-post-one.adoc"));
  await access(join(postsDir, "fixture-post-two.adoc"));
  await access(join(permalinksDir, `${fixturePosts[0].public_id}.md`));
  await access(join(permalinksDir, `${fixturePosts[1].public_id}.md`));

  const generated = await readFile(join(postsDir, "fixture-post-one.adoc"), "utf8");
  if (!generated.includes(`public_id: "${fixturePosts[0].public_id}"`)) {
    throw new Error("Generated fixture post is missing the expected public_id frontmatter.");
  }
}

export async function assertFixtureRenderedOutput(): Promise<void> {
  await access(join(siteOutputDir, "index.html"));
  await access(join(siteOutputDir, "posts", "fixture-post-one", "index.html"));
  await access(join(siteOutputDir, "posts", "fixture-post-two", "index.html"));
  await access(join(siteOutputDir, "p", fixturePosts[0].public_id, "index.html"));
}

function isMissingPathError(error: unknown): error is NodeJS.ErrnoException {
  return Boolean(error && typeof error === "object" && "code" in error && error.code === "ENOENT");
}

function runCommand(command: string, args: string[], cwd: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: "inherit",
      env: process.env,
    });

    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${command} ${args.join(" ")} exited with code ${code}`));
      }
    });
  });
}
