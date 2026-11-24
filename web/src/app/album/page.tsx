"use client";

import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Copy, FolderOpen, Lock, Pencil, Play, X } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import { AlbumForm } from "@/components/app/album-form";
import { PublicHeader } from "@/components/app/app-shell";
import { Footer } from "@/components/app/logo";
import { ItemIcon } from "@/components/app/item-icon";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { usePath } from "@/hooks/use-param";
import { api, type Item, type PublicAlbum, usePageData } from "@/lib/api";
import { copy } from "@/lib/copy";
import { bytes } from "@/lib/format";

function Lightbox({ items, index, onIndex, onClose }: { items: Item[]; index: number; onIndex: (i: number) => void; onClose: () => void }) {
  const item = items[index];
  const go = useCallback((d: number) => onIndex((index + d + items.length) % items.length), [index, items.length, onIndex]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowRight") go(1);
      if (e.key === "ArrowLeft") go(-1);
    };
    window.addEventListener("keydown", key);
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", key);
      document.body.style.overflow = "";
    };
  }, [go, onClose]);
  const next = items[(index + 1) % items.length];
  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-black/95 text-white" onClick={onClose}>
      <div className="flex items-center gap-2 p-3" onClick={(e) => e.stopPropagation()}>
        <span className="min-w-0 flex-1 truncate text-sm">{item.name}</span>
        <span className="text-xs text-white/60 tabular-nums">
          {index + 1} / {items.length}
        </span>
        <Button variant="ghost" size="icon" className="text-white hover:bg-white/10 hover:text-white" render={<a href={item.url} target="_blank" rel="noreferrer" />} aria-label="Open">
          <FolderOpen />
        </Button>
        <Button variant="ghost" size="icon" className="text-white hover:bg-white/10 hover:text-white" onClick={onClose} aria-label="Close">
          <X />
        </Button>
      </div>
      <div className="relative flex min-h-0 flex-1 items-center justify-center p-2" onClick={(e) => e.stopPropagation()}>
        {item.type === "video" ? (
          <video key={item.id} src={item.raw_url} controls autoPlay playsInline className="max-h-full max-w-full" />
        ) : (
          // eslint-disable-next-line @next/next/no-img-element
          <img key={item.id} src={item.raw_url} alt={item.name} className="max-h-full max-w-full object-contain" />
        )}
        {items.length > 1 && (
          <>
            <button type="button" aria-label="Previous" onClick={() => go(-1)} className="absolute top-1/2 left-2 -translate-y-1/2 rounded-full bg-white/10 p-2 hover:bg-white/20">
              <ChevronLeft />
            </button>
            <button type="button" aria-label="Next" onClick={() => go(1)} className="absolute top-1/2 right-2 -translate-y-1/2 rounded-full bg-white/10 p-2 hover:bg-white/20">
              <ChevronRight />
            </button>
          </>
        )}
        {next?.type === "image" && <link rel="prefetch" href={next.raw_url} />}
      </div>
    </div>
  );
}

function AlbumView() {
  const path = usePath();
  const id = path?.split("/")[2] ?? "";
  const initial = usePageData<PublicAlbum>();
  const [refreshed, setRefreshed] = useState(false);
  const fromPage = initial?.id === id && !refreshed ? initial : null;
  const fetched = useQuery({ queryKey: ["public-album", id], queryFn: () => api<PublicAlbum>(`/public/albums/${id}`), enabled: !!id && !fromPage, retry: false });
  const album = fromPage ?? fetched.data ?? null;
  const missing = fetched.isError;
  const [open, setOpen] = useState<number | null>(null);
  const [editing, setEditing] = useState(false);
  const load = () => {
    setRefreshed(true);
    void fetched.refetch();
  };

  if (missing) return <p className="py-24 text-center text-muted-foreground">Album not found</p>;
  if (!album) return <Skeleton className="h-96 w-full rounded-lg" />;
  const media = album.items.filter((i) => (i.type === "image" || i.type === "video") && !i.locked);
  const total = album.items.reduce((s, i) => s + i.size, 0);
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="min-w-0">
          <h1 className="text-2xl font-semibold tracking-tight">{album.title}</h1>
          {album.description && <p className="mt-1 max-w-2xl text-sm whitespace-pre-line text-muted-foreground">{album.description}</p>}
          <p className="mt-1 text-xs text-muted-foreground">
            {album.items.length} files · {bytes(total)}
            {album.uploader && ` · by ${album.uploader.name}`}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => copy(album.url)}>
            <Copy /> Copy link
          </Button>
          {album.owner && (
            <>
              <Button variant="outline" size="sm" render={<Link href={`/files?album=${album.id}`} />}>
                <FolderOpen /> Manage
              </Button>
              <Button variant="outline" size="sm" onClick={() => setEditing(true)}>
                <Pencil /> Edit
              </Button>
            </>
          )}
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
        {album.items.map((i) => {
          const m = media.indexOf(i);
          const inner = (
            <>
              {i.thumb_url ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img src={i.thumb_url} alt={i.name} loading="lazy" decoding="async" width={i.width ?? undefined} height={i.height ?? undefined} className="absolute inset-0 size-full object-cover" />
              ) : (
                <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-surface p-3 text-muted-foreground">
                  {i.locked ? <Lock className="size-7" /> : <ItemIcon type={i.type} className="size-8" />}
                  <span className="max-w-full truncate text-xs">{i.name}</span>
                </div>
              )}
              {i.type === "video" && i.thumb_url && (
                <span className="absolute inset-0 flex items-center justify-center">
                  <span className="flex size-10 items-center justify-center rounded-full bg-black/50 text-white">
                    <Play className="size-4 fill-current" />
                  </span>
                </span>
              )}
            </>
          );
          return m >= 0 ? (
            <button key={i.id} type="button" onClick={() => setOpen(m)} className="checker relative block w-full overflow-hidden rounded-lg border pt-[75%] hover:border-foreground/25">
              {inner}
            </button>
          ) : (
            <a key={i.id} href={i.url} className="checker relative block w-full overflow-hidden rounded-lg border pt-[75%] hover:border-foreground/25">
              {inner}
            </a>
          );
        })}
      </div>
      {album.items.length === 0 && <p className="py-12 text-center text-sm text-muted-foreground">This album is empty</p>}
      {open !== null && media[open] && <Lightbox items={media} index={open} onIndex={setOpen} onClose={() => setOpen(null)} />}
      {editing && <AlbumForm album={album} open onClose={() => setEditing(false)} onSaved={load} />}
    </div>
  );
}

export default function AlbumPage() {
  return (
    <div className="flex min-h-svh flex-col">
      <PublicHeader />
      <main className="mx-auto w-full max-w-5xl flex-1 px-4 py-6">
        <AlbumView />
      </main>
      <Footer />
    </div>
  );
}
