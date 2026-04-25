import { mkdir, mkdtemp, rename, rm, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { join } from "node:path";

type PostSummary = {
  public_id: string;
  slug: string;
  title: string;
  summary: string | null;
  published_at: string | null;
};

type Post = {
  public_id: string;
  slug: string;
  title: string;
  summary: string | null;
  body_adoc: string;
  published_at: string | null;
};

const API_URL = (process.env.BLOG_API_URL || process.env.API_URL || "http://localhost:8787")
  .replace(/\/+$/, "");
const SRC_DIR = join(import.meta.dirname, "..", "src");
const POSTS_DIR = join(import.meta.dirname, "..", "src", "posts");
const PERMALINKS_DIR = join(import.meta.dirname, "..", "src", "permalinks");

async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`GET ${url} returned ${response.status}`);
  }

  return response.json() as Promise<T>;
}

const postSummaries = await fetchJson<PostSummary[]>(`${API_URL}/posts`);

const posts = await Promise.all(
  postSummaries.map(async (summary) => {
    const post = await fetchJson<Post>(
      `${API_URL}/posts/by-id/${encodeURIComponent(summary.public_id)}`,
    );

    return {
      ...post,
      public_id: summary.public_id,
      summary: summary.summary,
      published_at: summary.published_at,
      title: summary.title,
    };
  }),
);

const stagingRoot = await mkdtemp(join(SRC_DIR, ".fetch-staging-"));
const stagedPostsDir = join(stagingRoot, "posts");
const stagedPermalinksDir = join(stagingRoot, "permalinks");

try {
  await mkdir(stagedPostsDir, { recursive: true });
  await mkdir(stagedPermalinksDir, { recursive: true });

  for (const post of posts) {
    const lines = [
      "---",
      `title: ${JSON.stringify(post.title)}`,
      `public_id: ${JSON.stringify(post.public_id)}`,
      `slug: ${JSON.stringify(post.slug)}`,
    ];

    if (post.summary) {
      lines.push(`summary: ${JSON.stringify(post.summary)}`);
    }

    lines.push(`published_at: ${JSON.stringify(post.published_at)}`);
    lines.push("---");

    await writeFile(
      join(stagedPostsDir, `${post.slug}.adoc`),
      `${lines.join("\n")}\n${post.body_adoc}\n`,
    );

    const redirectFrontmatter = [
      "---",
      'layout: "redirect.njk"',
      `permalink: ${JSON.stringify(`/p/${post.public_id}/index.html`)}`,
      `redirect_to: ${JSON.stringify(`/posts/${post.slug}/`)}`,
      "eleventyExcludeFromCollections: true",
      "---",
      "",
    ].join("\n");

    await writeFile(
      join(stagedPermalinksDir, `${post.public_id}.md`),
      redirectFrontmatter,
    );
  }

  await replaceDirectory(stagedPostsDir, POSTS_DIR);
  await replaceDirectory(stagedPermalinksDir, PERMALINKS_DIR);
} finally {
  await rm(stagingRoot, { recursive: true, force: true });
}

console.log(`Fetched ${posts.length} post(s) to ${POSTS_DIR}`);

async function replaceDirectory(stagedDir: string, targetDir: string): Promise<void> {
  const backupDir = `${targetDir}.bak-${randomUUID()}`;
  let movedExisting = false;

  try {
    try {
      await rename(targetDir, backupDir);
      movedExisting = true;
    } catch (error) {
      if (!isMissingPathError(error)) {
        throw error;
      }
    }

    await rename(stagedDir, targetDir);

    if (movedExisting) {
      await rm(backupDir, { recursive: true, force: true });
    }
  } catch (error) {
    if (movedExisting) {
      await rename(backupDir, targetDir).catch(() => {});
    }
    throw error;
  }
}

function isMissingPathError(error: unknown): error is NodeJS.ErrnoException {
  return Boolean(error && typeof error === "object" && "code" in error && error.code === "ENOENT");
}
