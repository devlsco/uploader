"use client";

import { useInfiniteQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useState } from "react";

import { ItemCard } from "@/components/files/item-card";
import { ItemDialog } from "@/components/files/item-dialog";
import type { Item } from "@/lib/api";
import { itemsQuery } from "@/lib/queries";

/** The newest uploads under the drop zone (from the list that is loaded anyway). */
export function Recent() {
  const list = useInfiniteQuery(itemsQuery("all", "", null));
  const [open, setOpen] = useState<Item | null>(null);
  const items = list.data?.pages[0]?.items.slice(0, 6) ?? [];
  if (!items.length) return null;
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium">Recent</h2>
        <Link href="/files" className="text-sm text-muted-foreground hover:text-foreground">
          All files
        </Link>
      </div>
      <div className="grid grid-cols-3 gap-3 sm:grid-cols-6">
        {items.map((i) => (
          <ItemCard key={i.id} item={i} eager selecting={false} selected={false} onClick={setOpen} />
        ))}
      </div>
      <ItemDialog item={open} onClose={() => setOpen(null)} />
    </section>
  );
}
