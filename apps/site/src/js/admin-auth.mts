import Asciidoctor from "@asciidoctor/core";
import hljs from "highlight.js";
import katex from "katex";
import { initializeApp } from "https://www.gstatic.com/firebasejs/12.7.0/firebase-app.js";
import {
  GoogleAuthProvider,
  getAuth,
  inMemoryPersistence,
  setPersistence,
  signInWithPopup,
  signOut,
} from "https://www.gstatic.com/firebasejs/12.7.0/firebase-auth.js";

type PostStatus = "draft" | "private" | "public" | "trashed";
type StatusTone = "pending" | "success" | "error";
type WorkspaceView = "posts" | "editor";

type Post = {
  public_id: string;
  slug: string;
  title: string;
  summary: string | null;
  body_adoc: string;
  status: PostStatus;
  published_at: string | null;
  created_at: string;
  updated_at: string;
  revision_no: number;
};

type CreatePostPayload = {
  slug: string;
  title: string;
  summary: string | null;
  body_adoc: string;
  status: PostStatus;
};

type UpdatePostPayload = {
  slug: string;
  title: string;
  summary: string;
  body_adoc: string;
  status: PostStatus;
  revision_no: number;
};

type SessionStatusResponse = {
  authenticated: boolean;
};

type SessionIssuedResponse = {
  ok: boolean;
  session: string;
};

type ErrorResponse = {
  error: string;
};

type AdminAppConfig = {
  apiUrl: string;
  firebase: {
    apiKey: string;
    authDomain: string;
    appId: string;
    projectId: string;
  };
};

type ApiError = Error & {
  status: number;
};

type DraftPostForm = {
  slug: string;
  title: string;
  summary: string;
  body_adoc: string;
  status: PostStatus;
};

type AppState = {
  authenticated: boolean;
  posts: Post[];
  selectedPublicId: string | null;
  busy: boolean;
  activeView: WorkspaceView;
  previewTimer: number | null;
};

type UiRefs = {
  workspaceShell: HTMLElement | null;
  postsView: HTMLElement | null;
  editorView: HTMLElement | null;
  lockedShell: HTMLElement | null;
  sessionLabel: HTMLParagraphElement | null;
  workspaceTitle: HTMLHeadingElement | null;
  workspaceCopy: HTMLParagraphElement | null;
  postsTabButton: HTMLButtonElement | null;
  editorTabButton: HTMLButtonElement | null;
  signInButton: HTMLButtonElement | null;
  refreshSessionButton: HTMLButtonElement | null;
  clearSessionButton: HTMLButtonElement | null;
  refreshPostsButton: HTMLButtonElement | null;
  newPostButton: HTMLButtonElement | null;
  savePostButton: HTMLButtonElement | null;
  trashPostButton: HTMLButtonElement | null;
  statusNode: HTMLParagraphElement | null;
  editorCaption: HTMLParagraphElement | null;
  previewCaption: HTMLParagraphElement | null;
  openPublicLink: HTMLAnchorElement | null;
  postFilter: HTMLSelectElement | null;
  postList: HTMLTableSectionElement | null;
  postForm: HTMLFormElement | null;
  sourceSlug: HTMLInputElement | null;
  sourcePublicId: HTMLInputElement | null;
  slugInput: HTMLInputElement | null;
  statusInput: HTMLSelectElement | null;
  titleInput: HTMLInputElement | null;
  summaryInput: HTMLTextAreaElement | null;
  bodyInput: HTMLTextAreaElement | null;
  revisionInput: HTMLInputElement | null;
  publicIdInput: HTMLInputElement | null;
  createdAtInput: HTMLInputElement | null;
  updatedAtInput: HTMLInputElement | null;
  previewTitle: HTMLHeadingElement | null;
  previewSummary: HTMLParagraphElement | null;
  previewBody: HTMLDivElement | null;
  previewStatus: HTMLParagraphElement | null;
};

const root = document.getElementById("admin-app");

if (!(root instanceof HTMLElement)) {
  throw new Error("Missing #admin-app root element.");
}

const ui: UiRefs = {
  workspaceShell: document.getElementById("admin-workspace"),
  postsView: document.getElementById("admin-posts-view"),
  editorView: document.getElementById("admin-editor-view"),
  lockedShell: document.getElementById("admin-locked-shell"),
  sessionLabel: document.getElementById("admin-session-label") as HTMLParagraphElement | null,
  workspaceTitle: document.getElementById("workspace-title") as HTMLHeadingElement | null,
  workspaceCopy: document.getElementById("workspace-copy") as HTMLParagraphElement | null,
  postsTabButton: document.getElementById("workspace-posts-button") as HTMLButtonElement | null,
  editorTabButton: document.getElementById("workspace-editor-button") as HTMLButtonElement | null,
  signInButton: document.getElementById("sign-in-button") as HTMLButtonElement | null,
  refreshSessionButton: document.getElementById("refresh-session-button") as HTMLButtonElement | null,
  clearSessionButton: document.getElementById("clear-session-button") as HTMLButtonElement | null,
  refreshPostsButton: document.getElementById("refresh-posts-button") as HTMLButtonElement | null,
  newPostButton: document.getElementById("new-post-button") as HTMLButtonElement | null,
  savePostButton: document.getElementById("save-post-button") as HTMLButtonElement | null,
  trashPostButton: document.getElementById("trash-post-button") as HTMLButtonElement | null,
  statusNode: document.getElementById("admin-status") as HTMLParagraphElement | null,
  editorCaption: document.getElementById("editor-caption") as HTMLParagraphElement | null,
  previewCaption: document.getElementById("preview-caption") as HTMLParagraphElement | null,
  openPublicLink: document.getElementById("open-public-link") as HTMLAnchorElement | null,
  postFilter: document.getElementById("post-filter") as HTMLSelectElement | null,
  postList: document.getElementById("post-list") as HTMLTableSectionElement | null,
  postForm: document.getElementById("post-form") as HTMLFormElement | null,
  sourceSlug: document.getElementById("source-slug") as HTMLInputElement | null,
  sourcePublicId: document.getElementById("source-public-id") as HTMLInputElement | null,
  slugInput: document.getElementById("slug-input") as HTMLInputElement | null,
  statusInput: document.getElementById("status-input") as HTMLSelectElement | null,
  titleInput: document.getElementById("title-input") as HTMLInputElement | null,
  summaryInput: document.getElementById("summary-input") as HTMLTextAreaElement | null,
  bodyInput: document.getElementById("body-input") as HTMLTextAreaElement | null,
  revisionInput: document.getElementById("revision-input") as HTMLInputElement | null,
  publicIdInput: document.getElementById("public-id-input") as HTMLInputElement | null,
  createdAtInput: document.getElementById("created-at-input") as HTMLInputElement | null,
  updatedAtInput: document.getElementById("updated-at-input") as HTMLInputElement | null,
  previewTitle: document.getElementById("preview-title") as HTMLHeadingElement | null,
  previewSummary: document.getElementById("preview-summary") as HTMLParagraphElement | null,
  previewBody: document.getElementById("preview-body") as HTMLDivElement | null,
  previewStatus: document.getElementById("preview-status-chip") as HTMLParagraphElement | null,
};

const config: AdminAppConfig = {
  apiUrl: root.dataset.apiUrl || "",
  firebase: {
    apiKey: root.dataset.firebaseApiKey || "",
    authDomain: root.dataset.firebaseAuthDomain || "",
    appId: root.dataset.firebaseAppId || "",
    projectId: root.dataset.firebaseProjectId || "",
  },
};

const state: AppState = {
  authenticated: false,
  posts: [],
  selectedPublicId: null,
  busy: false,
  activeView: "posts",
  previewTimer: null,
};

const missingKeys = Object.entries(config.firebase)
  .filter(([, value]) => !value)
  .map(([key]) => key);

const asciidoctor = Asciidoctor();
let auth: unknown = null;
let provider: GoogleAuthProvider | null = null;

if (!config.apiUrl || missingKeys.length > 0) {
  setSessionLabel("locked", "Browser configuration is incomplete");
  setStatus(
    `Missing browser configuration: ${[
      !config.apiUrl ? "apiUrl" : null,
      ...missingKeys,
    ].filter(Boolean).join(", ")}.`,
    "error",
  );
  disableAllControls();
  renderPreview(true);
} else {
  const app = initializeApp(config.firebase);
  auth = getAuth(app);
  provider = new GoogleAuthProvider();
  provider.setCustomParameters({ prompt: "select_account" });

  wireEvents();
  resetEditor({ switchView: false });
  await refreshSession({ announce: false });
}

function wireEvents(): void {
  ui.signInButton?.addEventListener("click", () => {
    void handleSignIn();
  });
  ui.refreshSessionButton?.addEventListener("click", () => {
    void refreshSession({ announce: true });
  });
  ui.clearSessionButton?.addEventListener("click", () => {
    void handleClearSession();
  });
  ui.refreshPostsButton?.addEventListener("click", () => {
    void loadPosts({ announce: true });
  });
  ui.newPostButton?.addEventListener("click", () => {
    resetEditor({ switchView: true });
    setStatus("The editor was reset for a new draft.", "pending");
  });
  ui.postFilter?.addEventListener("change", renderPostTable);
  ui.postForm?.addEventListener("submit", (event) => {
    void handleSavePost(event);
  });
  ui.trashPostButton?.addEventListener("click", () => {
    void handleTrashPost();
  });
  ui.postsTabButton?.addEventListener("click", () => {
    setActiveView("posts");
  });
  ui.editorTabButton?.addEventListener("click", () => {
    setActiveView("editor");
  });

  for (const control of [ui.slugInput, ui.statusInput, ui.titleInput, ui.summaryInput, ui.bodyInput]) {
    control?.addEventListener("input", () => {
      schedulePreviewRender();
    });
    control?.addEventListener("change", () => {
      schedulePreviewRender();
    });
  }
}

async function handleSignIn(): Promise<void> {
  if (!auth || !provider) {
    setStatus("Firebase authentication is not configured in the browser.", "error");
    return;
  }

  setBusy(true);
  setStatus("Opening Google sign-in…", "pending");

  try {
    await setPersistence(auth, inMemoryPersistence);
    const result = await signInWithPopup(auth, provider);
    const idToken = await result.user.getIdToken(true);

    await apiFetch<SessionIssuedResponse>("/auth/firebase-session", {
      method: "POST",
      body: JSON.stringify({ id_token: idToken }),
    });

    await signOut(auth);
    await refreshSession({ announce: false });
    setStatus("The admin session cookie was issued and the workspace is now unlocked.", "success");
  } catch (error) {
    setStatus(normalizeError(error), "error");
  } finally {
    await signOut(auth).catch(() => {});
    setBusy(false);
  }
}

async function handleClearSession(): Promise<void> {
  setBusy(true);
  setStatus("Clearing the admin session cookie…", "pending");

  try {
    await apiFetch<void>("/auth/session", { method: "DELETE" });
    state.authenticated = false;
    state.posts = [];
    resetEditor({ switchView: false });
    renderPostTable();
    syncControls();
    setStatus("The admin session cookie was cleared.", "success");
  } catch (error) {
    setStatus(normalizeError(error), "error");
  } finally {
    setBusy(false);
  }
}

async function refreshSession({ announce }: { announce: boolean }): Promise<void> {
  setSessionLabel("checking", "Checking browser session…");

  try {
    const payload = await apiFetch<SessionStatusResponse>("/auth/session");
    state.authenticated = Boolean(payload?.authenticated);
    syncControls();

    if (state.authenticated) {
      await loadPosts({ announce: false });
      if (announce) {
        setStatus("The browser is authenticated and the administrative API is available.", "success");
      }
    } else {
      state.posts = [];
      resetEditor({ switchView: false });
      renderPostTable();
      if (announce) {
        setStatus("No authenticated admin session cookie is currently present.", "pending");
      }
    }
  } catch (error) {
    state.authenticated = false;
    state.posts = [];
    resetEditor({ switchView: false });
    syncControls();
    renderPostTable();
    if (announce) {
      setStatus(normalizeError(error), "error");
    }
  }
}

async function loadPosts({
  announce,
  selectPublicId = state.selectedPublicId,
}: {
  announce: boolean;
  selectPublicId?: string | null;
}): Promise<void> {
  if (!state.authenticated) {
    renderPostTable();
    return;
  }

  try {
    const posts = await apiFetch<Post[]>("/admin/posts");
    state.posts = Array.isArray(posts) ? posts : [];
    renderPostTable();

    if (selectPublicId) {
      const match = state.posts.find((post) => post.public_id === selectPublicId);
      if (match) {
        selectPost(match.public_id, { switchView: false });
      } else if (state.selectedPublicId === selectPublicId) {
        state.selectedPublicId = null;
      }
    }

    if (!state.selectedPublicId && state.posts.length > 0) {
      selectPost(state.posts[0].public_id, { switchView: false });
    }

    if (announce) {
      setStatus(`Loaded ${state.posts.length} post(s) from the authenticated administrative API.`, "success");
    }
  } catch (error) {
    if (isUnauthorizedError(error)) {
      state.authenticated = false;
      state.posts = [];
      resetEditor({ switchView: false });
      renderPostTable();
      syncControls();
    }
    if (announce) {
      setStatus(normalizeError(error), "error");
    }
  }
}

async function handleSavePost(event: Event): Promise<void> {
  event.preventDefault();

  if (!state.authenticated) {
    setStatus("An authenticated admin session is required before saving posts.", "error");
    return;
  }

  const draft = readDraftFromForm();
  const isExisting = Boolean(ui.sourceSlug?.value);

  setBusy(true);
  setStatus(isExisting ? "Saving post changes…" : "Creating a new post…", "pending");

  try {
    let post: Post;

    if (isExisting) {
      const payload: UpdatePostPayload = {
        slug: draft.slug,
        title: draft.title,
        summary: draft.summary,
        body_adoc: draft.body_adoc,
        status: draft.status,
        revision_no: Number(ui.revisionInput?.value || "0"),
      };

      post = await apiFetch<Post>(`/posts/by-slug/${encodeURIComponent(requireValue(ui.sourceSlug))}`, {
        method: "PUT",
        body: JSON.stringify(payload),
      });
    } else {
      const payload: CreatePostPayload = {
        slug: draft.slug,
        title: draft.title,
        summary: draft.summary || null,
        body_adoc: draft.body_adoc,
        status: draft.status,
      };

      post = await apiFetch<Post>("/posts", {
        method: "POST",
        body: JSON.stringify(payload),
      });
    }

    upsertPost(post);
    selectPost(post.public_id, { switchView: true });
    renderPostTable();
    setStatus(isExisting ? "The post was updated successfully." : "The post was created successfully.", "success");
  } catch (error) {
    if (isUnauthorizedError(error)) {
      state.authenticated = false;
      syncControls();
    }
    setStatus(normalizeError(error), "error");
  } finally {
    setBusy(false);
  }
}

async function handleTrashPost(): Promise<void> {
  const sourceSlug = ui.sourceSlug?.value || "";
  const revisionNo = Number(ui.revisionInput?.value || "0");

  if (!sourceSlug || !revisionNo) {
    setStatus("Load an existing post before moving it to the trash.", "error");
    return;
  }
  if (!state.authenticated) {
    setStatus("An authenticated admin session is required before trashing posts.", "error");
    return;
  }

  setBusy(true);
  setStatus("Moving the post to the trash…", "pending");

  try {
    await apiFetch<void>(
      `/posts/by-slug/${encodeURIComponent(sourceSlug)}?revision_no=${encodeURIComponent(revisionNo)}`,
      { method: "DELETE" },
    );

    const selectedPublicId = ui.sourcePublicId?.value || state.selectedPublicId;
    await loadPosts({ announce: false, selectPublicId: selectedPublicId || null });
    const refreshed = state.posts.find((post) => post.public_id === selectedPublicId);
    if (refreshed) {
      selectPost(refreshed.public_id, { switchView: true });
    } else {
      resetEditor({ switchView: true });
    }
    setStatus("The post was moved to the trash.", "success");
  } catch (error) {
    if (isUnauthorizedError(error)) {
      state.authenticated = false;
      syncControls();
    }
    setStatus(normalizeError(error), "error");
  } finally {
    setBusy(false);
  }
}

function renderPostTable(): void {
  if (!ui.postList) return;

  if (!state.authenticated) {
    ui.postList.innerHTML = '<tr><td class="admin-empty" colspan="6">Sign in to load the administrative post list.</td></tr>';
    return;
  }

  const filter = ui.postFilter?.value || "all";
  const visiblePosts = state.posts.filter((post) => filter === "all" || post.status === filter);

  if (visiblePosts.length === 0) {
    ui.postList.innerHTML = '<tr><td class="admin-empty" colspan="6">No posts match the current filter.</td></tr>';
    return;
  }

  ui.postList.innerHTML = visiblePosts
    .map((post) => {
      const activeClass = post.public_id === state.selectedPublicId ? " class=\"is-active\"" : "";
      const publishedAt = post.published_at ? formatTimestamp(post.published_at) : "Unpublished";
      const summary = post.summary
        ? `<span class="admin-post-row-summary">${escapeHtml(post.summary)}</span>`
        : "";

      return `
        <tr${activeClass}>
          <td>
            <div class="admin-post-row-title">
              <strong>${escapeHtml(post.title)}</strong>
              ${summary}
            </div>
          </td>
          <td><code>${escapeHtml(post.slug)}</code></td>
          <td>${escapeHtml(post.status)}</td>
          <td>${escapeHtml(publishedAt)}</td>
          <td>r${escapeHtml(String(post.revision_no))}</td>
          <td class="admin-post-row-action">
            <button type="button" class="admin-post-row-button" data-public-id="${escapeHtml(post.public_id)}">Open</button>
          </td>
        </tr>
      `;
    })
    .join("");

  ui.postList.querySelectorAll<HTMLButtonElement>("[data-public-id]").forEach((button) => {
    button.addEventListener("click", () => {
      selectPost(button.getAttribute("data-public-id"), { switchView: true });
    });
  });
}

function selectPost(publicId: string | null, options: { switchView: boolean }): void {
  if (!publicId) return;

  const post = state.posts.find((item) => item.public_id === publicId);
  if (!post) return;

  state.selectedPublicId = post.public_id;
  setInputValue(ui.sourceSlug, post.slug);
  setInputValue(ui.sourcePublicId, post.public_id);
  setInputValue(ui.slugInput, post.slug);
  setSelectValue(ui.statusInput, post.status);
  setInputValue(ui.titleInput, post.title);
  setTextAreaValue(ui.summaryInput, post.summary || "");
  setTextAreaValue(ui.bodyInput, post.body_adoc);
  setInputValue(ui.revisionInput, String(post.revision_no));
  setInputValue(ui.publicIdInput, post.public_id);
  setInputValue(ui.createdAtInput, formatTimestamp(post.created_at));
  setInputValue(ui.updatedAtInput, formatTimestamp(post.updated_at));

  if (ui.editorCaption) {
    ui.editorCaption.textContent = `Editing ${post.slug} (${post.status}).`;
  }
  if (ui.savePostButton) {
    ui.savePostButton.textContent = "Save Changes";
  }
  if (ui.trashPostButton) {
    ui.trashPostButton.disabled = state.busy || !state.authenticated;
  }

  if (ui.openPublicLink) {
    if (post.status === "public") {
      ui.openPublicLink.hidden = false;
      ui.openPublicLink.href = `/posts/${post.slug}/`;
    } else {
      ui.openPublicLink.hidden = true;
      ui.openPublicLink.removeAttribute("href");
    }
  }

  renderPostTable();
  renderPreview(false);
  if (options.switchView) {
    setActiveView("editor");
  }
}

function resetEditor({ switchView }: { switchView: boolean }): void {
  state.selectedPublicId = null;
  setInputValue(ui.sourceSlug, "");
  setInputValue(ui.sourcePublicId, "");
  setInputValue(ui.slugInput, "");
  setSelectValue(ui.statusInput, "draft");
  setInputValue(ui.titleInput, "");
  setTextAreaValue(ui.summaryInput, "");
  setTextAreaValue(ui.bodyInput, "");
  setInputValue(ui.revisionInput, "");
  setInputValue(ui.publicIdInput, "");
  setInputValue(ui.createdAtInput, "");
  setInputValue(ui.updatedAtInput, "");

  if (ui.editorCaption) {
    ui.editorCaption.textContent = "Draft a post and review the rendered result live beside the editor.";
  }
  if (ui.savePostButton) {
    ui.savePostButton.textContent = "Create Post";
  }
  if (ui.trashPostButton) {
    ui.trashPostButton.disabled = true;
  }
  if (ui.openPublicLink) {
    ui.openPublicLink.hidden = true;
    ui.openPublicLink.removeAttribute("href");
  }

  renderPreview(true);
  renderPostTable();
  if (switchView) {
    setActiveView("editor");
  }
}

function renderPreview(isNewDraft: boolean): void {
  const draft = readDraftFromFormSafe();

  if (ui.previewTitle) {
    ui.previewTitle.textContent = draft?.title || "Untitled post";
  }
  if (ui.previewStatus) {
    ui.previewStatus.textContent = draft?.status || "draft";
  }
  if (ui.previewSummary) {
    const summary = draft?.summary.trim() || "";
    ui.previewSummary.hidden = summary.length === 0;
    ui.previewSummary.textContent = summary;
  }
  if (ui.previewCaption) {
    ui.previewCaption.textContent = isNewDraft
      ? "Rendered preview updates as you type."
      : "Rendered preview of the current editor state.";
  }

  if (!ui.previewBody) return;
  if (!draft || !draft.body_adoc.trim()) {
    ui.previewBody.innerHTML = '<p class="admin-empty">The live rendered preview will appear here.</p>';
    return;
  }

  try {
    const html = renderPreviewHtml(draft.body_adoc);
    ui.previewBody.innerHTML = html || '<p class="admin-empty">The rendered preview is empty.</p>';
  } catch (error) {
    ui.previewBody.innerHTML = `<p class="admin-empty">${escapeHtml(normalizeError(error))}</p>`;
  }
}

function schedulePreviewRender(): void {
  if (state.previewTimer !== null) {
    window.clearTimeout(state.previewTimer);
  }

  state.previewTimer = window.setTimeout(() => {
    renderPreview(false);
    state.previewTimer = null;
  }, 120);
}

function renderPreviewHtml(bodyAdoc: string): string {
  let html = String(asciidoctor.convert(bodyAdoc, {
    safe: "safe",
    attributes: {
      showtitle: false,
      stem: "latexmath",
    },
  }));

  html = highlightCode(html);
  html = sanitizeRenderedHtml(html);
  html = renderMath(html);
  return html;
}

function highlightCode(html: string): string {
  return html.replace(
    /<code class="language-([\w+#.-]+)"[^>]*>([\s\S]*?)<\/code>/g,
    (_, lang: string, code: string) => {
      const decoded = decodeEntities(code);
      if (hljs.getLanguage(lang)) {
        const result = hljs.highlight(decoded, { language: lang });
        return `<code class="language-${lang} hljs">${result.value}</code>`;
      }
      return `<code class="language-${lang}">${code}</code>`;
    },
  );
}

function renderMath(html: string): string {
  const parts = html.split(/(<pre[\s\S]*?<\/pre>|<code[\s\S]*?<\/code>)/gi);
  for (let index = 0; index < parts.length; index += 1) {
    if (index % 2 === 1) continue;

    parts[index] = parts[index].replace(/\\\[([\s\S]*?)\\\]/g, (_, tex: string) => {
      return katex.renderToString(decodeEntities(tex.trim()), {
        displayMode: true,
        throwOnError: false,
      });
    });

    parts[index] = parts[index].replace(/\\\(([\s\S]*?)\\\)/g, (_, tex: string) => {
      return katex.renderToString(decodeEntities(tex.trim()), {
        displayMode: false,
        throwOnError: false,
      });
    });
  }

  return parts.join("");
}

function sanitizeRenderedHtml(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, "")
    .replace(/<(iframe|object|embed|meta|link|base)\b[\s\S]*?(?:<\/\1>|\/?>)/gi, "")
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

function decodeEntities(value: string): string {
  return value
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, "\"")
    .replace(/&#39;/g, "'");
}

function readDraftFromForm(): DraftPostForm {
  return {
    slug: requireValue(ui.slugInput).trim(),
    title: requireValue(ui.titleInput).trim(),
    summary: ui.summaryInput?.value || "",
    body_adoc: requireValue(ui.bodyInput),
    status: requireSelectValue(ui.statusInput) as PostStatus,
  };
}

function readDraftFromFormSafe(): DraftPostForm | null {
  if (!ui.slugInput || !ui.titleInput || !ui.bodyInput || !ui.statusInput) {
    return null;
  }

  return {
    slug: ui.slugInput.value.trim(),
    title: ui.titleInput.value.trim(),
    summary: ui.summaryInput?.value || "",
    body_adoc: ui.bodyInput.value,
    status: ui.statusInput.value as PostStatus,
  };
}

function upsertPost(post: Post): void {
  const index = state.posts.findIndex((item) => item.public_id === post.public_id);
  if (index >= 0) {
    state.posts[index] = post;
  } else {
    state.posts.push(post);
  }

  state.posts.sort((a, b) => {
    return (b.updated_at || "").localeCompare(a.updated_at || "")
      || (b.created_at || "").localeCompare(a.created_at || "");
  });
}

async function apiFetch<T>(
  path: string,
  options: { method?: string; body?: string; headers?: Record<string, string> } = {},
): Promise<T> {
  const response = await fetch(`${config.apiUrl}${path}`, {
    method: options.method || "GET",
    credentials: "include",
    headers: {
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...(options.headers || {}),
    },
    body: options.body,
  });

  const payload = await readJsonSafely<T | ErrorResponse>(response);

  if (!response.ok) {
    const error = new Error(
      isErrorResponse(payload) ? payload.error : `${response.status} ${response.statusText}`.trim(),
    ) as ApiError;
    error.status = response.status;
    throw error;
  }

  return payload as T;
}

function setActiveView(view: WorkspaceView): void {
  state.activeView = view;

  if (ui.postsView) {
    ui.postsView.hidden = view !== "posts";
  }
  if (ui.editorView) {
    ui.editorView.hidden = view !== "editor";
  }
  if (ui.postsTabButton) {
    ui.postsTabButton.setAttribute("aria-selected", String(view === "posts"));
  }
  if (ui.editorTabButton) {
    ui.editorTabButton.setAttribute("aria-selected", String(view === "editor"));
  }
  if (ui.workspaceTitle) {
    ui.workspaceTitle.textContent = view === "posts" ? "Posts" : "Editor";
  }
  if (ui.workspaceCopy) {
    ui.workspaceCopy.textContent = view === "posts"
      ? "Review the administrative listing, filter by status, and open a post in the editor."
      : "Edit the canonical fields on the left and review the rendered result live on the right.";
  }
}

function disableAllControls(): void {
  for (const control of [
    ui.signInButton,
    ui.refreshSessionButton,
    ui.clearSessionButton,
    ui.refreshPostsButton,
    ui.newPostButton,
    ui.savePostButton,
    ui.trashPostButton,
    ui.postFilter,
    ui.postsTabButton,
    ui.editorTabButton,
    ui.slugInput,
    ui.statusInput,
    ui.titleInput,
    ui.summaryInput,
    ui.bodyInput,
  ]) {
    control?.setAttribute("disabled", "disabled");
  }
}

function syncControls(): void {
  if (!state.authenticated) {
    if (ui.workspaceShell) ui.workspaceShell.hidden = true;
    if (ui.lockedShell) ui.lockedShell.hidden = false;
    setSessionLabel("locked", "No authenticated admin session");
    return;
  }

  if (ui.workspaceShell) ui.workspaceShell.hidden = false;
  if (ui.lockedShell) ui.lockedShell.hidden = true;
  setSessionLabel("authenticated", "Authenticated workspace unlocked");

  const editorDisabled = state.busy || !state.authenticated;

  setControlDisabled(ui.signInButton, state.busy);
  setControlDisabled(ui.refreshSessionButton, state.busy);
  setControlDisabled(ui.clearSessionButton, state.busy || !state.authenticated);
  setControlDisabled(ui.refreshPostsButton, state.busy || !state.authenticated);
  setControlDisabled(ui.newPostButton, state.busy || !state.authenticated);
  setControlDisabled(ui.postFilter, state.busy || !state.authenticated);
  setControlDisabled(ui.postsTabButton, state.busy || !state.authenticated);
  setControlDisabled(ui.editorTabButton, state.busy || !state.authenticated);

  for (const control of [ui.slugInput, ui.statusInput, ui.titleInput, ui.summaryInput, ui.bodyInput]) {
    setControlDisabled(control, editorDisabled);
  }

  setControlDisabled(ui.savePostButton, editorDisabled);
  setControlDisabled(ui.trashPostButton, editorDisabled || !Boolean(ui.sourceSlug?.value));
}

function setBusy(isBusy: boolean): void {
  state.busy = isBusy;
  syncControls();
}

function setControlDisabled(
  control: HTMLButtonElement | HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement | null,
  disabled: boolean,
): void {
  if (!control) return;
  control.disabled = disabled;
  control.setAttribute("aria-busy", String(state.busy));
}

function setStatus(message: string, tone: StatusTone): void {
  if (!ui.statusNode) return;
  ui.statusNode.textContent = message;
  ui.statusNode.dataset.tone = tone;
}

function setSessionLabel(stateName: "checking" | "locked" | "authenticated", message: string): void {
  if (!ui.sessionLabel) return;
  ui.sessionLabel.dataset.state = stateName;
  ui.sessionLabel.textContent = message;
}

async function readJsonSafely<T>(response: Response): Promise<T | null> {
  const contentType = response.headers.get("content-type") || "";
  if (!contentType.includes("application/json")) {
    return null;
  }

  try {
    return await response.json() as T;
  } catch {
    return null;
  }
}

function normalizeError(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  return "The administrative workflow failed.";
}

function isUnauthorizedError(error: unknown): error is ApiError {
  return error instanceof Error && "status" in error && [401, 403].includes((error as ApiError).status);
}

function isErrorResponse(payload: unknown): payload is ErrorResponse {
  return typeof payload === "object" && payload !== null && "error" in payload;
}

function formatTimestamp(value: string): string {
  try {
    return new Date(value).toLocaleString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      timeZone: "UTC",
    });
  } catch {
    return value;
  }
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\"", "&quot;")
    .replaceAll("'", "&#39;");
}

function setInputValue(input: HTMLInputElement | null, value: string): void {
  if (input) {
    input.value = value;
  }
}

function setTextAreaValue(input: HTMLTextAreaElement | null, value: string): void {
  if (input) {
    input.value = value;
  }
}

function setSelectValue(input: HTMLSelectElement | null, value: string): void {
  if (input) {
    input.value = value;
  }
}

function requireValue(input: HTMLInputElement | HTMLTextAreaElement | null): string {
  if (!input) {
    throw new Error("A required form control is missing.");
  }

  return input.value;
}

function requireSelectValue(input: HTMLSelectElement | null): string {
  if (!input) {
    throw new Error("A required select control is missing.");
  }

  return input.value;
}
