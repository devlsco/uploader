"use client";

import { useSyncExternalStore } from "react";

const subscribe = (cb: () => void) => {
  window.addEventListener("popstate", cb);
  return () => window.removeEventListener("popstate", cb);
};

/** A query string value (static export: read in the browser, null while rendering on the server). */
export function useParam(name: string): string | null {
  return useSyncExternalStore(
    subscribe,
    () => new URLSearchParams(window.location.search).get(name),
    () => null,
  );
}

/** The current path in the browser ("/abc123" on item pages). */
export function usePath(): string | null {
  return useSyncExternalStore(
    subscribe,
    () => window.location.pathname,
    () => null,
  );
}
