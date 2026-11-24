import type { InfiniteData, QueryClient } from "@tanstack/react-query";

import type { Item } from "./api";
import type { ItemsPage } from "./queries";

type Pages = InfiniteData<ItemsPage, string>;

export function updateItem(qc: QueryClient, item: Item) {
  qc.setQueriesData<Pages>({ queryKey: ["items"] }, (d) => d && { ...d, pages: d.pages.map((p) => ({ ...p, items: p.items.map((i) => (i.id === item.id ? item : i)) })) });
}

export function removeItems(qc: QueryClient, ids: string[]) {
  const gone = new Set(ids);
  qc.setQueriesData<Pages>({ queryKey: ["items"] }, (d) => d && { ...d, pages: d.pages.map((p) => ({ ...p, items: p.items.filter((i) => !gone.has(i.id)) })) });
  void qc.invalidateQueries({ queryKey: ["stats"] });
  void qc.invalidateQueries({ queryKey: ["albums"] });
}

let timer: ReturnType<typeof setTimeout> | undefined;

/** After uploads: refresh the lists once, shortly after the last one finished. */
export function refreshSoon(qc: QueryClient) {
  clearTimeout(timer);
  timer = setTimeout(() => {
    void qc.invalidateQueries({ queryKey: ["items"] });
    void qc.invalidateQueries({ queryKey: ["stats"] });
    void qc.invalidateQueries({ queryKey: ["albums"] });
  }, 600);
}
