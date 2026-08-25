const DEFAULT_API_URL = "http://localhost:4000";

export function getApiBaseUrl(): string {
  if (typeof window !== "undefined") {
    return process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL;
  }
  return process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL;
}

let sessionToken: string | null = null;
let sessionPromise: Promise<string> | null = null;

export async function getSessionToken(): Promise<string> {
  if (sessionToken) return sessionToken;

  if (typeof window !== "undefined") {
    const stored = window.sessionStorage.getItem("ember_demo_session_token");
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
          window.sessionStorage.setItem("ember_demo_session_token", data.token);
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
  const token = await getSessionToken();
  const baseUrl = getApiBaseUrl();
  const url = `${baseUrl}${path.startsWith("/") ? path : `/${path}`}`;

  const headers = new Headers(options.headers || {});
  headers.set("Authorization", `Bearer ${token}`);
  if (!headers.has("Content-Type") && options.body && typeof options.body === "string") {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(url, {
    ...options,
    headers,
  });

  if (!response.ok) {
    const errorBody = (await response.json().catch(() => null)) as { error?: string } | null;
    throw new Error(errorBody?.error || `API request failed with status ${response.status}`);
  }

  return response.json() as Promise<T>;
}
