"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FolderOpen, Images, Pencil, Plus, Trash2 } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { toast } from "sonner";

import { AlbumForm } from "@/components/app/album-form";
import { PageHeader } from "@/components/app/app-shell";
import { useAlbums } from "@/components/app/upload-options";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Empty, EmptyContent, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type Album } from "@/lib/api";
import { copy } from "@/lib/copy";
import { bytes } from "@/lib/format";

function Albums() {
  const albums = useAlbums();
  const qc = useQueryClient();
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Album | null>(null);
  const del = useMutation({
    mutationFn: (id: string) => api(`/albums/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["albums"] });
      toast.success("Album deleted, the files are kept");
    },
  });
  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Albums"
        description="Group files and share them with one link."
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus /> New album
          </Button>
        }
      />
      {albums.isPending ? (
        <div className="grid grid-cols-2 gap-3 md:grid-cols-3 lg:grid-cols-4">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} className="aspect-square rounded-lg" />
          ))}
        </div>
      ) : !albums.data?.length ? (
        <Empty className="rounded-lg border border-dashed py-16">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Images />
            </EmptyMedia>
            <EmptyTitle>No albums yet</EmptyTitle>
          </EmptyHeader>
          <EmptyContent>
            <Button onClick={() => setCreating(true)}>
              <Plus /> New album
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <div className="grid grid-cols-2 gap-3 md:grid-cols-3 lg:grid-cols-4">
          {albums.data.map((a) => (
            <div key={a.id} className="group relative overflow-hidden rounded-lg border bg-card hover:border-foreground/25">
              <Link href={`/files?album=${a.id}`} className="block">
                <div className="grid aspect-[4/3] grid-cols-2 grid-rows-2 gap-px bg-muted">
                  {a.covers.length === 0 ? (
                    <div className="col-span-2 row-span-2 flex items-center justify-center bg-surface text-muted-foreground">
                      <Images className="size-8" />
                    </div>
                  ) : (
                    a.covers.slice(0, 4).map((c, i) => (
                      // eslint-disable-next-line @next/next/no-img-element
                      <img key={c} src={`/${c}/thumb`} alt="" loading="lazy" className={`size-full object-cover ${a.covers.length === 1 ? "col-span-2 row-span-2" : a.covers.length === 2 ? "row-span-2" : a.covers.length === 3 && i === 0 ? "row-span-2" : ""}`} />
                    ))
                  )}
                </div>
                <div className="px-3 py-2">
                  <p className="truncate text-sm font-medium">{a.title}</p>
                  <p className="text-xs text-muted-foreground">
                    {a.count} files · {bytes(a.size)}
                    {!a.public && " · Private"}
                  </p>
                </div>
              </Link>
              <div className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
                <DropdownMenu>
                  <DropdownMenuTrigger render={<Button variant="secondary" size="icon-sm" aria-label="Album actions" />}>
                    <Pencil />
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => window.open(a.url, "_blank")}>
                      <ExternalLink /> Open page
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => copy(a.url)}>
                      <FolderOpen /> Copy link
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => setEditing(a)}>
                      <Pencil /> Edit
                    </DropdownMenuItem>
                    <DropdownMenuItem variant="destructive" onClick={() => del.mutate(a.id)}>
                      <Trash2 /> Delete
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>
          ))}
        </div>
      )}
      {creating && <AlbumForm open onClose={() => setCreating(false)} />}
      {editing && <AlbumForm album={editing} open onClose={() => setEditing(null)} />}
    </div>
  );
}

export default function AlbumsPage() {
  return <Albums />;
}
