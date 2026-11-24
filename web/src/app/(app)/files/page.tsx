"use client";

import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { CheckSquare, FolderInput, Search, Trash2, X } from "lucide-react";
import Link from "next/link";
import { useCallback, useDeferredValue, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { PageHeader } from "@/components/app/app-shell";
import { useAlbums } from "@/components/app/upload-options";
import { ItemCard } from "@/components/files/item-card";
import { ItemDialog } from "@/components/files/item-dialog";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useParam } from "@/hooks/use-param";
import { cn } from "cn";
import { api, type Item } from "@/lib/api";
import { removeItems } from "@/lib/cache";
import { itemsQuery } from "@/lib/queries";

const TYPES = [
  { value: "all", label: "All" },
  { value: "image", label: "Images" },
  { value: "video", label: "Videos" },
  { value: "audio", label: "Audio" },
  { value: "pdf", label: "PDFs" },
  { value: "text", label: "Text" },
  { value: "link", label: "Links" },
  { value: "other", label: "Other" },
];

function Files() {
  const qc = useQueryClient();
  const albumParam = useParam("album");
  const [type, setType] = useState("all");
  const [search, setSearch] = useState("");
  const q = useDeferredValue(search.trim());
  const [open, setOpen] = useState<Item | null>(null);
  const [selecting, setSelecting] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const albums = useAlbums();
  const album = albums.data?.find((a) => a.id === albumParam);

  const list = useInfiniteQuery(itemsQuery(type, q, albumParam));
  const items = list.data?.pages.flatMap((p) => p.items) ?? [];

  const searchRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if (e.key === "Escape" && !typing && selecting) {
        setSelecting(false);
        setSelected(new Set());
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [selecting]);
  const sentinel = useRef<HTMLDivElement>(null);
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = list;
  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver((e) => e[0].isIntersecting && hasNextPage && !isFetchingNextPage && fetchNextPage(), { rootMargin: "600px" });
    io.observe(el);
    return () => io.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const click = useCallback(
    (item: Item) => {
      if (!selecting) return setOpen(item);
      setSelected((s) => {
        const n = new Set(s);
        if (n.has(item.id)) n.delete(item.id);
        else n.add(item.id);
        return n;
      });
    },
    [selecting],
  );
  const done = () => {
    setSelected(new Set());
    setSelecting(false);
    void qc.invalidateQueries({ queryKey: ["items"] });
    void qc.invalidateQueries({ queryKey: ["albums"] });
    void qc.invalidateQueries({ queryKey: ["stats"] });
  };
  const del = useMutation({
    mutationFn: () => api<{ deleted: number }>("/items/delete", { body: { ids: [...selected] } }),
    onSuccess: (r) => {
      toast.success(`${r.deleted} deleted`);
      removeItems(qc, [...selected]);
      setSelected(new Set());
      setSelecting(false);
    },
    onError: (e) => toast.error(e.message),
  });
  const move = useMutation({
    mutationFn: (album_id: string | null) => api<{ moved: number }>("/items/album", { body: { ids: [...selected], album_id } }),
    onSuccess: (r) => {
      toast.success(`${r.moved} moved`);
      done();
    },
    onError: (e) => toast.error(e.message),
  });

  return (
    <div className="flex flex-col gap-5 pb-20">
      <PageHeader
        title={album ? album.title : "Files"}
        description={
          album ? (
            <Link href="/files" className="inline-flex items-center gap-1 hover:text-foreground">
              <X className="size-3.5" /> Show all files
            </Link>
          ) : (
            "Everything you uploaded, newest first."
          )
        }
        actions={
          <>
            <div className="relative w-full sm:w-64">
              <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input ref={searchRef} placeholder="Search  /" value={search} onChange={(e) => setSearch(e.target.value)} className="bg-card pl-9" />
            </div>
            <Button variant={selecting ? "secondary" : "outline"} onClick={() => (selecting ? (setSelecting(false), setSelected(new Set())) : setSelecting(true))}>
              <CheckSquare /> {selecting ? "Done" : "Select"}
            </Button>
          </>
        }
      />
      <div className="-mx-4 overflow-x-auto px-4 [scrollbar-width:none]">
        <div className="flex w-max gap-1 border-b">
          {TYPES.map((t) => (
            <button
              key={t.value}
              type="button"
              onClick={() => setType(t.value)}
              className={cn(
                "-mb-px border-b-2 border-transparent px-2.5 pb-2 text-sm text-muted-foreground transition-colors hover:text-foreground",
                type === t.value && "border-foreground text-foreground",
              )}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      {list.isPending ? (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
          {Array.from({ length: 12 }, (_, i) => (
            <Skeleton key={i} className="aspect-[4/5] rounded-lg" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <Empty className="rounded-lg border border-dashed py-16">
          <EmptyHeader>
            <EmptyTitle>Nothing here yet</EmptyTitle>
            <EmptyDescription>{q || type !== "all" ? "Try another filter." : <Link href="/" className="underline">Upload something</Link>}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
          {items.map((i, n) => (
            <ItemCard key={i.id} item={i} eager={n < 12} selecting={selecting} selected={selected.has(i.id)} onClick={click} />
          ))}
        </div>
      )}
      <div ref={sentinel} className="h-1" />
      {isFetchingNextPage && <p className="text-center text-sm text-muted-foreground">Loading…</p>}

      {selecting && selected.size > 0 && (
        <div className="fixed inset-x-0 bottom-20 z-40 sm:bottom-6 flex justify-center px-4">
          <div className="flex items-center gap-2 rounded-lg border bg-popover p-1.5 pl-3 shadow-lg">
            <span className="pr-2 text-sm font-medium">{selected.size} selected</span>
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="outline" size="sm" />}>
                <FolderInput /> Album
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuItem onClick={() => move.mutate(null)}>No album</DropdownMenuItem>
                {(albums.data ?? []).map((a) => (
                  <DropdownMenuItem key={a.id} onClick={() => move.mutate(a.id)}>
                    {a.title}
                  </DropdownMenuItem>
                ))}
              </DropdownMenuContent>
            </DropdownMenu>
            <Button variant="destructive" size="sm" disabled={del.isPending} onClick={() => del.mutate()}>
              <Trash2 /> Delete
            </Button>
          </div>
        </div>
      )}
      <ItemDialog item={open} onClose={() => setOpen(null)} />
    </div>
  );
}

export default function FilesPage() {
  return <Files />;
}
