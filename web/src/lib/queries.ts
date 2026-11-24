import type { QueryClient } from "@tanstack/react-query";

import { type Album, api, type Item } from "./api";

export type ItemsPage = { items: Item[]; next: string | null };

export function itemsQuery(type: string, q: string, album: string | null) {
  return {
    queryKey: ["items", type, q, album] as const,
    queryFn: ({ pageParam, signal }: { pageParam: string; signal?: AbortSignal }) => {
      const p = new URLSearchParams();
      if (type !== "all") p.set("type", type);
      if (q) p.set("q", q);
      if (album) p.set("album", album);
      if (pageParam) p.set("before", pageParam);
      return api<ItemsPage>(`/items?${p}`, { signal });
    },
    initialPageParam: "",
    getNextPageParam: (last: ItemsPage) => last.next ?? undefined,
  };
}

export const albumsQuery = { queryKey: ["albums"], queryFn: () => api<Album[]>("/albums") };

export function statsQuery(scope: string, days: string) {
  return { queryKey: ["stats", scope, days], queryFn: () => api(`/stats?days=${days}${scope === "all" ? "&scope=all" : ""}`) };
}

/** Loads what the other pages show first, while the browser is idle. */
export function preload(qc: QueryClient) {
  const run = () => {
    void qc.prefetchInfiniteQuery(itemsQuery("all", "", null));
    void qc.prefetchQuery(albumsQuery);
    void qc.prefetchQuery(statsQuery("me", "30"));
  };
  if ("requestIdleCallback" in window) window.requestIdleCallback(run, { timeout: 2000 });
  else setTimeout(run, 300);
}
