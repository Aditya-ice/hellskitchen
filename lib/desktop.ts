"use client";

import { useSyncExternalStore } from "react";

/**
 * The small amount of the UI that knows it might be running inside the macOS
 * app. Everything here degrades to a no-op in a browser tab, so the same build
 * serves both.
 */

interface TauriGlobal {
  event?: {
    listen: (
      event: string,
      handler: (payload: { payload: unknown }) => void,
    ) => Promise<() => void>;
  };
}

declare global {
  interface Window {
    __TAURI__?: TauriGlobal;
  }
}

export function isDesktop(): boolean {
  return typeof window !== "undefined" && window.__TAURI__ !== undefined;
}

/**
 * Subscribes to menu-driven tab changes (⌘1–⌘4). Returns an unsubscribe.
 *
 * The listener is registered asynchronously, so the returned function has to
 * cope with being called before registration finishes.
 */
export function onDesktopTabChange(
  handler: (tab: string) => void,
): () => void {
  const listen = typeof window !== "undefined" ? window.__TAURI__?.event?.listen : undefined;
  if (!listen) return () => {};

  let stop: (() => void) | null = null;
  let cancelled = false;

  listen("ember://tab", (event) => {
    if (typeof event.payload === "string") handler(event.payload);
  })
    .then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    })
    .catch(() => {
      // Running outside the desktop shell, or without the event permission.
    });

  return () => {
    cancelled = true;
    stop?.();
  };
}

export type Surface = "pos" | "kitchen";

/** Which surface this window should render. */
export function surfaceFromLocation(): Surface {
  if (typeof window === "undefined") return "pos";
  return new URLSearchParams(window.location.search).get("view") === "kitchen"
    ? "kitchen"
    : "pos";
}

const noopSubscribe = () => () => {};
const serverSurface = (): Surface => "pos";

/**
 * Reads the surface from the URL without tripping hydration.
 *
 * The bundle is statically exported, so the prerendered HTML is always the POS;
 * `useSyncExternalStore` is the supported way to let the client disagree with
 * that on first paint.
 */
export function useSurface(): Surface {
  return useSyncExternalStore(noopSubscribe, surfaceFromLocation, serverSurface);
}
