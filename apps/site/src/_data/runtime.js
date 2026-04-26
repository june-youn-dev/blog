const allowLocalDefaults = process.env.BLOG_ALLOW_LOCAL_DEFAULTS === "1";

function parseAdminEnabledFlag() {
  const raw = (process.env.BLOG_ENABLE_ADMIN || "").trim().toLowerCase();
  return ["1", "true", "yes", "on"].includes(raw);
}

function readRequiredEnv(name) {
  const value = (process.env[name] || "").trim();
  if (!value) {
    throw new Error(`${name} is required for a production site build.`);
  }
  return value;
}

function readApiUrl() {
  const configured = (process.env.BLOG_API_URL || process.env.API_URL || "").trim();
  if (configured) {
    return configured.replace(/\/+$/, "");
  }
  if (allowLocalDefaults) {
    return "http://localhost:8787";
  }
  throw new Error("BLOG_API_URL (or API_URL) is required for a production site build.");
}

const apiUrl = readApiUrl();
const adminEnabled = parseAdminEnabledFlag();
const firebaseProjectId = adminEnabled
  ? (allowLocalDefaults
      ? (process.env.FIREBASE_PROJECT_ID || "").trim()
      : readRequiredEnv("FIREBASE_PROJECT_ID"))
  : "";
const firebaseApiKey = adminEnabled
  ? (allowLocalDefaults
      ? (process.env.FIREBASE_WEB_API_KEY || "").trim()
      : readRequiredEnv("FIREBASE_WEB_API_KEY"))
  : "";
const firebaseAppId = adminEnabled
  ? (allowLocalDefaults
      ? (process.env.FIREBASE_APP_ID || "").trim()
      : readRequiredEnv("FIREBASE_APP_ID"))
  : "";

export default {
  adminEnabled,
  apiUrl,
  firebase: {
    apiKey: firebaseApiKey,
    authDomain: adminEnabled
      ? (process.env.FIREBASE_AUTH_DOMAIN || `${firebaseProjectId}.firebaseapp.com`).trim()
      : "",
    appId: firebaseAppId,
    projectId: firebaseProjectId,
  },
};
