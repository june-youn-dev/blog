declare module "https://www.gstatic.com/firebasejs/12.7.0/firebase-app.js" {
  export function initializeApp(config: Record<string, string>): unknown;
}

declare module "https://www.gstatic.com/firebasejs/12.7.0/firebase-auth.js" {
  export class GoogleAuthProvider {
    setCustomParameters(parameters: Record<string, string>): void;
  }

  export const inMemoryPersistence: unknown;

  export function getAuth(app?: unknown): unknown;
  export function setPersistence(auth: unknown, persistence: unknown): Promise<void>;
  export function signInWithPopup(
    auth: unknown,
    provider: GoogleAuthProvider,
  ): Promise<{
    user: {
      getIdToken(forceRefresh?: boolean): Promise<string>;
    };
  }>;
  export function signOut(auth: unknown): Promise<void>;
}
