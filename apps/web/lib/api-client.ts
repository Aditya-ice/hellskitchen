const DEFAULT_API_URL = "http://localhost:4000";

export function getApiBaseUrl(): string {
  if (typeof window !== "undefined") {
    return process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL;
  }
  return process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL;
}

let sessionToken: string | null = null;
let sessionPromise: Promise<string> | null = null;
const SESSION_STORAGE_KEY = "ember_demo_session_token";

export function clearSessionToken(expectedToken?: string) {
  if (expectedToken && sessionToken && sessionToken !== expectedToken) return;

  sessionToken = null;
  if (typeof window !== "undefined") {
    const stored = window.sessionStorage.getItem(SESSION_STORAGE_KEY);
    if (!expectedToken || !stored || stored === expectedToken) {
      window.sessionStorage.removeItem(SESSION_STORAGE_KEY);
    }
  }
}

export async function getSessionToken(): Promise<string> {
  if (sessionToken) return sessionToken;

  if (typeof window !== "undefined") {
    const stored = window.sessionStorage.getItem(SESSION_STORAGE_KEY);
    if (stored) {
      sessionToken = stored;
      return stored;
    }
  }

  if (!sessionPromise) {
    const baseUrl = getApiBaseUrl();
    sessionPromise = fetch(`${baseUrl}/v1/session`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    })
      .then(async (res) => {
        if (!res.ok) {
          throw new Error("Unable to create demo session.");
        }
        const data = (await res.json()) as { token: string };
        sessionToken = data.token;
        if (typeof window !== "undefined") {
          window.sessionStorage.setItem(SESSION_STORAGE_KEY, data.token);
        }
        return data.token;
      })
      .finally(() => {
        sessionPromise = null;
      });
  }

  return sessionPromise;
}

export async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const baseUrl = getApiBaseUrl();
  const url = `${baseUrl}${path.startsWith("/") ? path : `/${path}`}`;

  const headers = new Headers(options.headers || {});
  if (!headers.has("Content-Type") && options.body && typeof options.body === "string") {
    headers.set("Content-Type", "application/json");
  }

  async function send(token: string) {
    const requestHeaders = new Headers(headers);
    requestHeaders.set("Authorization", `Bearer ${token}`);
    return fetch(url, {
      ...options,
      headers: requestHeaders,
    });
  }

  const token = await getSessionToken();
  let response = await send(token);

  if (response.status === 401) {
    clearSessionToken(token);
    response = await send(await getSessionToken());
  }

  if (!response.ok) {
    const errorBody = (await response.json().catch(() => null)) as { error?: string } | null;
    throw new Error(errorBody?.error || `API request failed with status ${response.status}`);
  }

  return response.json() as Promise<T>;
}
