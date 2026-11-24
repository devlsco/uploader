"use client";

import { Check, Clock, Copy, Flame, Lock, Play } from "lucide-react";
import { memo } from "react";

import { ItemIcon } from "@/components/app/item-icon";
import type { Item } from "@/lib/api";
import { copy } from "@/lib/copy";
import { ago, bytes, duration } from "@/lib/format";
import { cn } from "cn";

export const ItemCard = memo(function ItemCard({ item, selected, selecting, eager, onClick }: { item: Item; selected: boolean; selecting: boolean; eager?: boolean; onClick: (item: Item) => void }) {
  return (
    <button
      type="button"
      onClick={() => onClick(item)}
      className={cn("group flex min-w-0 flex-col overflow-hidden rounded-lg border bg-card text-left hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none", selected && "border-foreground ring-1 ring-foreground")}
      style={{ contentVisibility: "auto", containIntrinsicSize: "220px" }}
    >
      <div className="checker relative w-full shrink-0 overflow-hidden pt-[75%]">
        {item.thumb_url ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img src={item.thumb_url} alt="" loading={eager ? "eager" : "lazy"} fetchPriority={eager ? "high" : "auto"} decoding="async" className="absolute inset-0 size-full object-cover" />
        ) : (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-surface px-3 text-muted-foreground">
            <span className="flex size-10 items-center justify-center rounded-md border bg-background">
              <ItemIcon type={item.type} className="size-4" />
            </span>
            {item.kind === "link" && <span className="max-w-full truncate text-xs">{item.name}</span>}
          </div>
        )}
        {item.type === "video" && item.thumb_url && (
          <span className="absolute inset-0 flex items-center justify-center">
            <span className="flex size-9 items-center justify-center rounded-full bg-black/50 text-white">
              <Play className="size-4 fill-current" />
            </span>
          </span>
        )}
        {!selecting && (
          <span
            role="button"
            tabIndex={-1}
            aria-label="Copy link"
            onClick={(e) => {
              e.stopPropagation();
              void copy(item.url);
            }}
            className="absolute bottom-1.5 left-1.5 flex size-7 items-center justify-center rounded-md border bg-background/90 text-foreground opacity-0 shadow-xs group-hover:opacity-100 hover:bg-background"
          >
            <Copy className="size-3.5" />
          </span>
        )}
        <div className="absolute top-1.5 right-1.5 flex gap-1">
          {item.locked && <Lock className="size-5 rounded bg-black/55 p-1 text-white" />}
          {item.burn && <Flame className="size-5 rounded bg-black/55 p-1 text-white" />}
          {item.expires_at && <Clock className="size-5 rounded bg-black/55 p-1 text-white" />}
        </div>
        {item.duration ? <span className="absolute right-1.5 bottom-1.5 rounded bg-black/60 px-1 text-[11px] text-white tabular-nums">{duration(item.duration)}</span> : null}
        {selecting && (
          <span className={cn("absolute top-1.5 left-1.5 flex size-5 items-center justify-center rounded border-2 border-white bg-black/30 text-white", selected && "border-foreground bg-foreground text-background")}>
            {selected && <Check className="size-3.5" />}
          </span>
        )}
      </div>
      <div className="flex flex-col gap-0.5 px-2.5 py-2">
        <span className="truncate text-sm font-medium">{item.name}</span>
        <span className="truncate text-xs text-muted-foreground">
          {item.kind === "link" ? `${item.views} clicks` : bytes(item.size)} · {ago(item.created_at)}
        </span>
      </div>
    </button>
  );
});
