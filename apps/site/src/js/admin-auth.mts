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

type AppState = {
  authenticated: boolean;
  posts: Post[];
  selectedPublicId: string | null;
  busy: boolean;
};

type UiRefs = {
  signInButton: HTMLButtonElement | null;
  refreshSessionButton: HTMLButtonElement | null;
  clearSessionButton: HTMLButtonElement | null;
  refreshPostsButton: HTMLButtonElement | null;
  newPostButton: HTMLButtonElement | null;
  savePostButton: HTMLButtonElement | null;
  trashPostButton: HTMLButtonElement | null;
  statusNode: HTMLParagraphElement | null;
  editorCaption: HTMLParagraphElement | null;
  openPublicLink: HTMLAnchorElement | null;
  postFilter: HTMLSelectElement | null;
  postList: HTMLUListElement | null;
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
};

type DraftPostForm = {
  slug: string;
  title: string;
  summary: string;
  body_adoc: string;
  status: PostStatus;
};

const root = document.getElementById("admin-app");

if (!(root instanceof HTMLElement)) {
  throw new Error("Missing #admin-app root element.");
}

const ui: UiRefs = {
  signInButton: document.getElementById("sign-in-button") as HTMLButtonElement | null,
  refreshSessionButton: document.getElementById("refresh-session-button") as HTMLButtonElement | null,
  clearSessionButton: document.getElementById("clear-session-button") as HTMLButtonElement | null,
  refreshPostsButton: document.getElementById("refresh-posts-button") as HTMLButtonElement | null,
  newPostButton: document.getElementById("new-post-button") as HTMLButtonElement | null,
  savePostButton: document.getElementById("save-post-button") as HTMLButtonElement | null,
  trashPostButton: document.getElementById("trash-post-button") as HTMLButtonElement | null,
  statusNode: document.getElementById("admin-status") as HTMLParagraphElement | null,
  editorCaption: document.getElementById("editor-caption") as HTMLParagraphElement | null,
  openPublicLink: document.getElementById("open-public-link") as HTMLAnchorElement | null,
  postFilter: document.getElementById("post-filter") as HTMLSelectElement | null,
  postList: document.getElementById("post-list") as HTMLUListElement | null,
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
};

const missingKeys = Object.entries(config.firebase)
  .filter(([, value]) => !value)
  .map(([key]) => key);

let auth: unknown = null;
let provider: GoogleAuthProvider | null = null;

if (!config.apiUrl || missingKeys.length > 0) {
  setStatus(
    `Missing browser configuration: ${[
      !config.apiUrl ? "apiUrl" : null,
      ...missingKeys,
    ].filter(Boolean).join(", ")}.`,
    "error",
  );
  disableAllControls();
} else {
  const app = initializeApp(config.firebase);
  auth = getAuth(app);
  provider = new GoogleAuthProvider();
  provider.setCustomParameters({ prompt: "select_account" });

  wireEvents();
  resetEditor();
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
    resetEditor();
    setStatus("The editor was reset for a new post.", "pending");
  });
  ui.postFilter?.addEventListener("change", renderPostList);
  ui.postForm?.addEventListener("submit", (event) => {
    void handleSavePost(event);
  });
  ui.trashPostButton?.addEventListener("click", () => {
    void handleTrashPost();
  });
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
    await loadPosts({ announce: false });
    setStatus(
      "The admin session cookie was issued successfully. The administrative post list is now available.",
      "success",
    );
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
    resetEditor();
    renderPostList();
    syncControls();
    setStatus("The admin session cookie was cleared.", "success");
  } catch (error) {
    setStatus(normalizeError(error), "error");
  } finally {
    setBusy(false);
  }
}

async function refreshSession({ announce }: { announce: boolean }): Promise<void> {
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
      resetEditor();
      renderPostList();
      if (announce) {
        setStatus("No authenticated admin session cookie is currently present.", "pending");
      }
    }
  } catch (error) {
    state.authenticated = false;
    syncControls();
    state.posts = [];
    renderPostList();
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
    renderPostList();
    return;
  }

  try {
    const posts = await apiFetch<Post[]>("/admin/posts");
    state.posts = Array.isArray(posts) ? posts : [];
    renderPostList();

    if (selectPublicId) {
      const match = state.posts.find((post) => post.public_id === selectPublicId);
      if (match) {
        selectPost(match.public_id);
      } else if (state.selectedPublicId === selectPublicId) {
        state.selectedPublicId = null;
      }
    }

    if (!state.selectedPublicId && state.posts.length > 0) {
      selectPost(state.posts[0].public_id);
    }

    if (announce) {
      setStatus(`Loaded ${state.posts.length} post(s) from the authenticated administrative API.`, "success");
    }
  } catch (error) {
    if (isUnauthorizedError(error)) {
      state.authenticated = false;
      state.posts = [];
      resetEditor();
      renderPostList();
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
    selectPost(post.public_id);
    renderPostList();
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
      selectPost(refreshed.public_id);
    } else {
      resetEditor();
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

function renderPostList(): void {
  if (!ui.postList) return;

  if (!state.authenticated) {
    ui.postList.innerHTML = '<li class="admin-empty">Sign in to load the administrative post list.</li>';
    return;
  }

  const filter = ui.postFilter?.value || "all";
  const visiblePosts = state.posts.filter((post) => filter === "all" || post.status === filter);

  if (visiblePosts.length === 0) {
    ui.postList.innerHTML = '<li class="admin-empty">No posts match the current filter.</li>';
    return;
  }

  ui.postList.innerHTML = visiblePosts
    .map((post) => {
      const activeClass = post.public_id === state.selectedPublicId ? " is-active" : "";
      const publishedAt = post.published_at ? formatTimestamp(post.published_at) : "Unpublished";
      const summary = escapeHtml(post.summary || "");
      return `
        <li>
          <button type="button" class="admin-post-card${activeClass}" data-public-id="${escapeHtml(post.public_id)}">
            <span class="admin-post-card-title">${escapeHtml(post.title)}</span>
            <span class="admin-post-card-meta">${escapeHtml(post.slug)} · ${escapeHtml(post.status)} · r${escapeHtml(String(post.revision_no))}</span>
            <span class="admin-post-card-meta">${escapeHtml(publishedAt)}</span>
            ${summary ? `<span class="admin-post-card-summary">${summary}</span>` : ""}
          </button>
        </li>
      `;
    })
    .join("");

  ui.postList.querySelectorAll<HTMLButtonElement>("[data-public-id]").forEach((button) => {
    button.addEventListener("click", () => {
      selectPost(button.getAttribute("data-public-id"));
    });
  });
}

function selectPost(publicId: string | null): void {
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
  setInputValue(ui.createdAtInput, post.created_at);
  setInputValue(ui.updatedAtInput, post.updated_at);

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

  renderPostList();
}

function resetEditor(): void {
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
    ui.editorCaption.textContent = "Create a new post or load an existing post from the list.";
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

async function apiFetch<T>(path: string, options: { method?: string; body?: string; headers?: Record<string, string> } = {}): Promise<T> {
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
    resetEditor();
  }

  const editorDisabled = state.busy || !state.authenticated;

  setControlDisabled(ui.signInButton, state.busy);
  setControlDisabled(ui.refreshSessionButton, state.busy);
  setControlDisabled(ui.clearSessionButton, state.busy || !state.authenticated);
  setControlDisabled(ui.refreshPostsButton, state.busy || !state.authenticated);
  setControlDisabled(ui.newPostButton, state.busy || !state.authenticated);
  setControlDisabled(ui.postFilter, state.busy || !state.authenticated);

  for (const control of [
    ui.slugInput,
    ui.statusInput,
    ui.titleInput,
    ui.summaryInput,
    ui.bodyInput,
  ]) {
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
    .replaceAll('"', "&quot;")
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

function requireValue(
  input: HTMLInputElement | HTMLTextAreaElement | null,
): string {
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
