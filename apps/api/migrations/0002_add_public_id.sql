-- Migration: 0002_add_public_id
-- Purpose: Add a stable public UUID identifier to posts so that
--          canonical friendly slugs may change without breaking
--          long-lived external links.

PRAGMA foreign_keys = OFF;

CREATE TABLE posts_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    summary TEXT,
    body_adoc TEXT NOT NULL,
    status TEXT NOT NULL
    CHECK (status IN ('draft', 'private', 'public', 'trashed')),
    published_at TEXT,
    created_at TEXT NOT NULL
    DEFAULT (STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL
    DEFAULT (STRFTIME('%Y-%m-%dT%H:%M:%SZ', 'now')),
    revision_no INTEGER NOT NULL DEFAULT 1
);

INSERT INTO posts_new (
    id,
    public_id,
    slug,
    title,
    summary,
    body_adoc,
    status,
    published_at,
    created_at,
    updated_at,
    revision_no
)
SELECT
    id,
    LOWER(
        HEX(RANDOMBLOB(4)) || '-'
        || HEX(RANDOMBLOB(2)) || '-'
        || '4' || SUBSTR(HEX(RANDOMBLOB(2)), 2) || '-'
        || SUBSTR('89AB', (ABS(RANDOM()) % 4) + 1, 1) || SUBSTR(HEX(RANDOMBLOB(2)), 2) || '-'
        || HEX(RANDOMBLOB(6))
    ) AS public_id,
    slug,
    title,
    summary,
    body_adoc,
    status,
    published_at,
    created_at,
    updated_at,
    revision_no
FROM posts;

DROP TABLE posts;
ALTER TABLE posts_new RENAME TO posts;

CREATE INDEX idx_posts_status ON posts (status);
CREATE INDEX idx_posts_published_at ON posts (published_at);

PRAGMA foreign_keys = ON;
