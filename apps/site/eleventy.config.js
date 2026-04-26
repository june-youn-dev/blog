import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import Asciidoctor from "@asciidoctor/core";
import hljs from "highlight.js";
import katex from "katex";

const __dirname = dirname(fileURLToPath(import.meta.url));
const CSS_ASSET_KEYS = ["css/base.css", "css/site.css", "css/admin.css"];

const asciidoctor = Asciidoctor();

// ── Post-processing helpers ──────────────────────────────────────
// These run on the HTML that Asciidoctor produces, BEFORE the
// result is handed to the layout.  The goal is to keep all
// heavy rendering at build time so the browser ships zero JS.

function decodeEntities(str) {
  return str
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

function assertSafeAsciiDoc(inputContent) {
  if (hasUnsafePassthroughBlock(inputContent)) {
    throw new Error("Raw HTML passthrough blocks (`++++`) are not allowed.");
  }

  if (/pass:[^\s]*\[/i.test(inputContent)) {
    throw new Error("AsciiDoc pass macros are not allowed.");
  }

  if (/(?:link|image|xref):\s*(?:javascript|vbscript|data):/i.test(inputContent)) {
    throw new Error("Unsafe macro targets are not allowed.");
  }
}

function hasUnsafePassthroughBlock(inputContent) {
  const lines = inputContent.split(/\r?\n/);
  let awaitingStemBlock = false;
  let inStemBlock = false;

  for (const line of lines) {
    const trimmed = line.trim();

    if (trimmed.startsWith("[stem")) {
      awaitingStemBlock = true;
      continue;
    }

    if (trimmed === "++++") {
      if (awaitingStemBlock) {
        awaitingStemBlock = false;
        inStemBlock = true;
        continue;
      }

      if (inStemBlock) {
        inStemBlock = false;
        continue;
      }

      return true;
    }

    if (trimmed && awaitingStemBlock) {
      awaitingStemBlock = false;
    }
  }

  return false;
}

function highlightCode(html) {
  return html.replace(
    /<code class="language-([\w+#.-]+)"[^>]*>([\s\S]*?)<\/code>/g,
    (_, lang, code) => {
      const decoded = decodeEntities(code);
      if (hljs.getLanguage(lang)) {
        const result = hljs.highlight(decoded, { language: lang });
        return `<code class="language-${lang} hljs">${result.value}</code>`;
      }
      return `<code class="language-${lang}">${code}</code>`;
    },
  );
}

function renderMath(html) {
  // Split the HTML into segments that are inside <pre>/<code> blocks
  // vs. outside them so that math delimiters inside code samples are
  // left untouched.
  const parts = html.split(/(<pre[\s\S]*?<\/pre>|<code[\s\S]*?<\/code>)/gi);
  for (let i = 0; i < parts.length; i++) {
    // Odd indices are the captured <pre>/<code> blocks — skip them.
    if (i % 2 === 1) continue;

    // Display math: \[...\]
    parts[i] = parts[i].replace(/\\\[([\s\S]*?)\\\]/g, (_, tex) => {
      return katex.renderToString(decodeEntities(tex.trim()), {
        displayMode: true,
        throwOnError: false,
      });
    });
    // Inline math: \(...\)
    parts[i] = parts[i].replace(/\\\(([\s\S]*?)\\\)/g, (_, tex) => {
      return katex.renderToString(decodeEntities(tex.trim()), {
        displayMode: false,
        throwOnError: false,
      });
    });
  }
  return parts.join("");
}

function sanitizeRenderedHtml(html) {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, "")
    .replace(/<(iframe|object|embed|meta|link|base)\b[\s\S]*?(?:<\/\1>|\/?>)/gi, "")
    .replace(/\sstyle\s*=\s*(".*?"|'.*?'|[^\s>]+)/gi, "")
    .replace(/\son[a-z-]+\s*=\s*(".*?"|'.*?'|[^\s>]+)/gi, "")
    .replace(/\s(href|src)\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]+))/gi, (_, attr, raw, dq, sq, bare) => {
      const value = dq ?? sq ?? bare ?? "";
      const normalized = value.trim().toLowerCase();
      if (
        normalized.startsWith("javascript:") ||
        normalized.startsWith("vbscript:") ||
        normalized.startsWith("data:")
      ) {
        return "";
      }
      return ` ${attr}=${raw}`;
    });
}

// ── Eleventy config ──────────────────────────────────────────────

export default function (eleventyConfig) {
  eleventyConfig.addTemplateFormats("adoc");
  eleventyConfig.addExtension("adoc", {
    compile(inputContent) {
      return function (data) {
        assertSafeAsciiDoc(inputContent);

        let html = asciidoctor.convert(inputContent, {
          safe: "safe",
          attributes: {
            showtitle: false,
            stem: "latexmath",
          },
        });

        const hasCode = /<code class="language-[\w+#.-]+"/.test(html);
        html = highlightCode(html);
        html = sanitizeRenderedHtml(html);

        const hasMath = /\\\[[\s\S]*?\\\]/.test(html) || /\\\([\s\S]*?\\\)/.test(html);
        html = renderMath(html);

        data.hasCode = hasCode;
        data.hasMath = hasMath;

        return html;
      };
    },
  });

  // Format an RFC 3339 timestamp as a human-readable date.
  eleventyConfig.addFilter("readableDate", (dateStr) => {
    return new Date(dateStr).toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
      timeZone: "UTC",
    });
  });

  // Content hashes for cache busting on first-party CSS files.
  const cssHashes = Object.fromEntries(
    CSS_ASSET_KEYS.map((key) => {
      const css = readFileSync(resolve(__dirname, "src", key), "utf8");
      return [key, createHash("md5").update(css).digest("hex").slice(0, 8)];
    }),
  );
  eleventyConfig.addGlobalData("cssHashes", cssHashes);

  // Inline small CSS files that are conditionally loaded.
  const hljsCss = readFileSync(
    resolve(__dirname, "node_modules/highlight.js/styles/github.min.css"),
    "utf8",
  );
  const katexCss = readFileSync(
    resolve(__dirname, "node_modules/katex/dist/katex.min.css"),
    "utf8",
  ).replaceAll("url(fonts/", "url(/css/fonts/");
  eleventyConfig.addFilter("inlineCss", (key) => {
    if (key === "css/hljs.css") return hljsCss;
    if (key === "css/katex.css") return katexCss;
    return "";
  });

  // Copy external CSS assets to the output directory.
  eleventyConfig.addPassthroughCopy("src/css");
  eleventyConfig.addPassthroughCopy({ "src/js/admin-auth.js": "js/admin-auth.js" });
  eleventyConfig.addPassthroughCopy({ "src/_headers": "_headers" });
  eleventyConfig.addPassthroughCopy({
    "node_modules/katex/dist/katex.min.css": "css/katex.min.css",
    "node_modules/katex/dist/fonts": "css/fonts",
  });

  // 11ty reads .gitignore by default, which excludes src/posts/ (the
  // build-generated content directory). Disable that so 11ty only
  // respects the explicit ignores below.
  eleventyConfig.setUseGitIgnore(false);
  eleventyConfig.ignores.add("src/api-types/**");

  return {
    dir: {
      input: "src",
      output: "_site",
      includes: "_includes",
      data: "_data",
    },
  };
}
