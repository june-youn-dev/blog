-- Migration: 0001_init
-- Purpose: Create the initial schema required for the MVP blog platform.
-- Target:  Cloudflare D1 (SQLite-compatible).
--
-- Conventions:
--   * All timestamp columns store RFC 3339 strings in UTC with a `Z`
--     suffix (e.g. `2026-04-12T02:04:09Z`). This format is what
--     `chrono::DateTime<Utc>` produces and consumes by default, so the
--     Rust side can use typed timestamps without a custom serde
--     adapter. Columns are declared as TEXT for SQLite compatibility
--     and given a DEFAULT that calls `strftime` with the RFC 3339
--     pattern so that INSERTs can omit them.
--   * Boolean-like flags are stored as INTEGER (0 or 1).
--   * Status columns are constrained with CHECK clauses so that invalid
--     values are rejected at the database level rather than silently
--     corrupting state.

-- -----------------------------------------------------------------------------
-- posts
--
-- Canonical post storage. The AsciiDoc source in `body_adoc` is authoritative;
-- all rendered artifacts (static HTML, RSS, search index, ...) are produced
-- downstream at build time and must be regenerable from `body_adoc` alone.
-- -----------------------------------------------------------------------------
CREATE TABLE posts (
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

CREATE INDEX idx_posts_status ON posts (status);
CREATE INDEX idx_posts_published_at ON posts (published_at);
