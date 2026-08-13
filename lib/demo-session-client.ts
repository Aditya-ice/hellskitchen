"use client";

let sessionPromise: Promise<void> | null = null;

export function ensureDemoSession() {
  if (!sessionPromise) {
    sessionPromise = fetch("/api/demo-session", {
      method: "POST",
      credentials: "same-origin",
    }).then(async (response) => {
      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as
          | { error?: string }
          | null;
        throw new Error(body?.error ?? "Unable to start the demo session.");
      }
    });
    sessionPromise.catch(() => {
      sessionPromise = null;
    });
  }
  return sessionPromise;
}
