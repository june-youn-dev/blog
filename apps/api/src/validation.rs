//! Shared input-validation helpers for the blog API.
//!
//! These checks are intentionally reusable across the HTTP edge and the
//! repository layer so that malformed requests can be rejected early
//! without letting the validation rules drift between modules.

use crate::dto::{CreatePost, UpdatePost};

pub(crate) const MAX_SLUG_LEN: usize = 96;
pub(crate) const MAX_BODY_SIZE: usize = 512 * 1024;
const MAX_TITLE_LEN: usize = 200;
const MAX_SUMMARY_LEN: usize = 2_000;

/// Validates a slug: must be 1-96 characters, lowercase ASCII
/// alphanumeric plus hyphens, and must not start or end with a hyphen.
pub(crate) fn validate_slug(slug: &str) -> Result<(), &'static str> {
    if slug.is_empty() || slug.len() > MAX_SLUG_LEN {
        return Err("slug must be between 1 and 96 characters");
    }
    if !slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
        return Err("slug must contain only lowercase ASCII letters, digits, and hyphens");
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("slug must not start or end with a hyphen");
    }
    if slug.contains("--") {
        return Err("slug must not contain consecutive hyphens");
    }
    Ok(())
}

pub(crate) fn validate_title(title: &str) -> Result<(), &'static str> {
    if title.trim().is_empty() {
        return Err("title must not be empty");
    }
    if title.chars().count() > MAX_TITLE_LEN {
        return Err("title must be at most 200 characters");
    }
    Ok(())
}

pub(crate) fn validate_summary(summary: &str) -> Result<(), &'static str> {
    if summary.chars().count() > MAX_SUMMARY_LEN {
        return Err("summary must be at most 2000 characters");
    }
    Ok(())
}

pub(crate) fn validate_revision_no(revision_no: i64) -> Result<(), &'static str> {
    if revision_no <= 0 { Err("revision_no must be positive") } else { Ok(()) }
}

/// Rejects AsciiDoc constructs that can inject executable HTML or
/// script-bearing URLs into downstream renderers.
pub(crate) fn validate_body_adoc(body: &str) -> Result<(), &'static str> {
    if body.len() > MAX_BODY_SIZE {
        return Err("body_adoc exceeds 524288 byte limit");
    }

    if has_unsafe_passthrough_block(body) {
        return Err("raw HTML passthrough blocks (`++++`) are not allowed");
    }

    let lower = body.to_ascii_lowercase();

    if contains_inline_pass_macro(&lower) {
        return Err("AsciiDoc pass macros are not allowed");
    }

    if contains_unsafe_macro_target(&lower) {
        return Err("javascript:, vbscript:, and data: macro targets are not allowed");
    }

    Ok(())
}

pub(crate) fn validate_create_post_input(input: &CreatePost) -> Result<(), &'static str> {
    validate_slug(&input.slug)?;
    validate_title(&input.title)?;
    if let Some(summary) = &input.summary {
        validate_summary(summary)?;
    }
    validate_body_adoc(&input.body_adoc)?;
    Ok(())
}

pub(crate) fn validate_update_post_input(
    slug: &str,
    input: &UpdatePost,
) -> Result<(), &'static str> {
    validate_slug(slug)?;
    validate_revision_no(input.revision_no)?;
    if let Some(next_slug) = &input.slug {
        validate_slug(next_slug)?;
    }
    if input.title.is_none()
        && input.slug.is_none()
        && input.summary.is_none()
        && input.body_adoc.is_none()
        && input.status.is_none()
    {
        return Err("at least one field must be provided");
    }
    if let Some(title) = &input.title {
        validate_title(title)?;
    }
    if let Some(summary) = &input.summary
        && !summary.is_empty()
    {
        validate_summary(summary)?;
    }
    if let Some(body) = &input.body_adoc {
        validate_body_adoc(body)?;
    }
    Ok(())
}

fn has_unsafe_passthrough_block(body: &str) -> bool {
    let mut awaiting_stem_block = false;
    let mut in_stem_block = false;

    for line in body.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("[stem") {
            awaiting_stem_block = true;
            continue;
        }

        if trimmed == "++++" {
            if awaiting_stem_block {
                awaiting_stem_block = false;
                in_stem_block = true;
                continue;
            }

            if in_stem_block {
                in_stem_block = false;
                continue;
            }

            return true;
        }

        if !trimmed.is_empty() && awaiting_stem_block {
            awaiting_stem_block = false;
        }
    }

    false
}

fn contains_inline_pass_macro(lower: &str) -> bool {
    let mut cursor = lower;
    while let Some(pos) = cursor.find("pass:") {
        let rest = &cursor[pos + "pass:".len()..];
        let boundary = rest
            .char_indices()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(rest.len());

        if rest[..boundary].contains('[') {
            return true;
        }

        cursor = rest;
    }

    false
}

fn contains_unsafe_macro_target(lower: &str) -> bool {
    ["link:", "image:", "xref:"].into_iter().any(|prefix| {
        let mut cursor = lower;
        while let Some(pos) = cursor.find(prefix) {
            let rest = &cursor[pos + prefix.len()..];
            let target = rest.split('[').next().unwrap_or(rest).trim_start();
            if target.starts_with("javascript:")
                || target.starts_with("vbscript:")
                || target.starts_with("data:")
            {
                return true;
            }
            cursor = rest;
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_BODY_SIZE, MAX_SLUG_LEN, validate_body_adoc, validate_create_post_input,
        validate_revision_no, validate_slug, validate_summary, validate_title,
        validate_update_post_input,
    };
    use crate::dto::{CreatePost, PostStatus, UpdatePost};

    #[test]
    fn rejects_invalid_slugs() {
        assert!(validate_slug("Bad Slug").is_err());
        assert!(validate_slug("-bad").is_err());
        assert!(validate_slug("bad-").is_err());
        assert!(validate_slug(&"a".repeat(MAX_SLUG_LEN + 1)).is_err());
        assert!(validate_slug(&"a".repeat(MAX_SLUG_LEN)).is_ok());
    }

    #[test]
    fn rejects_blank_titles() {
        assert!(validate_title("").is_err());
        assert!(validate_title("   ").is_err());
    }

    #[test]
    fn rejects_overlong_summaries() {
        let summary = "a".repeat(2001);
        assert!(validate_summary(&summary).is_err());
    }

    #[test]
    fn rejects_non_positive_revision_numbers() {
        assert!(validate_revision_no(0).is_err());
        assert!(validate_revision_no(-1).is_err());
    }

    #[test]
    fn validates_create_post_payload() {
        let payload = CreatePost {
            slug: "hello-world".into(),
            title: "Hello".into(),
            summary: Some("Summary".into()),
            body_adoc: "= Hello".into(),
            status: PostStatus::Public,
        };

        assert!(validate_create_post_input(&payload).is_ok());
    }

    #[test]
    fn rejects_oversized_bodies() {
        let body = "a".repeat(MAX_BODY_SIZE + 1);
        assert!(validate_body_adoc(&body).is_err());
    }

    #[test]
    fn rejects_passthrough_blocks() {
        let doc = "= Title\n\n++++\n<div onclick=alert(1)>x</div>\n++++\n";
        assert!(validate_body_adoc(doc).is_err());
    }

    #[test]
    fn allows_stem_passthrough_blocks() {
        let doc = "= Title\n\n[stem]\n++++\nx^2\n++++\n";
        assert!(validate_body_adoc(doc).is_ok());
    }

    #[test]
    fn rejects_unsafe_macro_targets() {
        assert!(validate_body_adoc("link:javascript:alert(1)[click]").is_err());
        assert!(validate_body_adoc("image:data:text/html,evil[payload]").is_err());
    }

    #[test]
    fn validates_update_post_payload() {
        let payload = UpdatePost {
            slug: None,
            title: Some("Updated".into()),
            summary: Some(String::new()),
            body_adoc: None,
            status: None,
            revision_no: 1,
        };

        assert!(validate_update_post_input("hello-world", &payload).is_ok());
    }

    #[test]
    fn rejects_empty_update_payloads() {
        let payload = UpdatePost {
            slug: None,
            title: None,
            summary: None,
            body_adoc: None,
            status: None,
            revision_no: 1,
        };

        assert!(validate_update_post_input("hello-world", &payload).is_err());
    }
}
