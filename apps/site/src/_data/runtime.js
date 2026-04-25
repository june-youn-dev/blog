const apiUrl = (process.env.BLOG_API_URL || process.env.API_URL || "http://localhost:8787")
  .replace(/\/+$/, "");
const firebaseProjectId = (process.env.FIREBASE_PROJECT_ID || "").trim();

export default {
  apiUrl,
  firebase: {
    apiKey: (process.env.FIREBASE_WEB_API_KEY || "").trim(),
    authDomain: (process.env.FIREBASE_AUTH_DOMAIN || `${firebaseProjectId}.firebaseapp.com`).trim(),
    appId: (process.env.FIREBASE_APP_ID || "").trim(),
    projectId: firebaseProjectId,
  },
};
