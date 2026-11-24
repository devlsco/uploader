"use client";

import { useQuery } from "@tanstack/react-query";
import { Copy, Download, ExternalLink, Flame, Lock, Pencil, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { PublicHeader } from "@/components/app/app-shell";
import { ItemIcon } from "@/components/app/item-icon";
import { Footer } from "@/components/app/logo";
import { ItemDialog } from "@/components/files/item-dialog";
import { QrButton } from "@/components/qr/qr-dialog";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { CodeView } from "@/components/view/code-view";
import { usePath } from "@/hooks/use-param";
import { api, type Item, type PublicItem, usePageData } from "@/lib/api";
import { copy } from "@/lib/copy";
import { ago, bytes, date, duration } from "@/lib/format";
import { cn } from "cn";

function Unlock({ item, onUnlocked }: { item: PublicItem; onUnlocked: (d: PublicItem) => void }) {
  const [pw, setPw] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  return (
    <Card className="mx-auto mt-12 w-full max-w-sm">
      <CardContent className="flex flex-col items-center gap-4 py-4 text-center">
        <span className="flex size-12 items-center justify-center rounded-md border bg-background">
          <Lock className="size-5" />
        </span>
        <p className="text-sm text-muted-foreground">This {item.kind === "link" ? "link" : "file"} is protected</p>
        <form
          className="flex w-full flex-col gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            setErr(null);
            try {
              const d = await api<PublicItem>(`/public/items/${item.id}/unlock`, { body: { password: pw } });
              if (d.kind === "link") window.location.replace(`/${item.id}`);
              else onUnlocked(d);
            } catch (er) {
              setErr((er as Error).message);
            } finally {
              setBusy(false);
            }
          }}
        >
          <Input autoFocus type="password" placeholder="Password" value={pw} onChange={(e) => setPw(e.target.value)} aria-invalid={!!err} />
          {err && <p className="text-sm text-destructive">{err}</p>}
          <Button type="submit" disabled={!pw || busy}>
            Open
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

function Burn({ onReveal }: { onReveal: () => void }) {
  return (
    <Card className="mx-auto mt-12 w-full max-w-sm">
      <CardContent className="flex flex-col items-center gap-4 py-4 text-center">
        <span className="flex size-12 items-center justify-center rounded-md border bg-background">
          <Flame className="size-5" />
        </span>
        <p className="text-sm text-muted-foreground">This can only be opened once. After that it is gone.</p>
        <Button onClick={onReveal}>Open now</Button>
      </CardContent>
    </Card>
  );
}

function TextContent({ item, src }: { item: PublicItem; src: string }) {
  const [text, setText] = useState<string | null>(item.content ?? null);
  const [err, setErr] = useState(false);
  const tooBig = (item.size ?? 0) > 8 * 1024 * 1024;
  useEffect(() => {
    if (item.content != null || tooBig) return;
    const ctl = new AbortController();
    fetch(src, { signal: ctl.signal })
      .then((r) => (r.ok ? r.text() : Promise.reject()))
      .then(setText)
      .catch(() => !ctl.signal.aborted && setErr(true));
    return () => ctl.abort();
  }, [item.content, src, tooBig]);
  if (tooBig) return <Other item={item} />;
  if (err) return <p className="text-center text-sm text-muted-foreground">Could not load the text.</p>;
  if (text == null) return <Skeleton className="h-96 w-full rounded-lg" />;
  return <CodeView code={text} name={item.name} language={item.language} mime={item.mime} />;
}

function Other({ item }: { item: PublicItem }) {
  return (
    <Card className="mx-auto w-full max-w-sm">
      <CardContent className="flex flex-col items-center gap-3 py-6 text-center">
        <ItemIcon type={item.type} className="size-12 text-muted-foreground" />
        <p className="max-w-full truncate font-medium">{item.name}</p>
        <p className="text-sm text-muted-foreground">{bytes(item.size)}</p>
        <Button render={<a href={item.download_url} />}>
          <Download /> Download
        </Button>
      </CardContent>
    </Card>
  );
}

function Media({ item, reveal }: { item: PublicItem; reveal: boolean }) {
  const [zoom, setZoom] = useState(false);
  const src = `${item.raw_url}${reveal ? "?reveal=1" : ""}`;
  switch (item.type) {
    case "image":
      return (
        <div className={cn("checker flex justify-center overflow-hidden rounded-lg border", zoom && "overflow-auto")}>
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={src}
            alt={item.name}
            width={item.width ?? undefined}
            height={item.height ?? undefined}
            onClick={() => setZoom(!zoom)}
            className={cn(zoom ? "max-w-none cursor-zoom-out" : "h-auto max-h-[78vh] w-auto max-w-full cursor-zoom-in object-contain")}
          />
        </div>
      );
    case "video":
      return <video src={src} controls autoPlay playsInline preload="metadata" poster={item.thumb_url ?? undefined} className="max-h-[78vh] w-full rounded-lg border bg-black" />;
    case "audio":
      return (
        <Card className="mx-auto w-full max-w-md">
          <CardContent className="flex flex-col items-center gap-4 py-2">
            {item.thumb_url ? (
              // eslint-disable-next-line @next/next/no-img-element
              <img src={item.thumb_url} alt="" className="size-48 rounded-lg object-cover shadow" />
            ) : (
              <ItemIcon type="audio" className="size-14 text-muted-foreground" />
            )}
            <p className="max-w-full truncate font-medium">{item.name}</p>
            <audio src={src} controls autoPlay preload="metadata" className="w-full" />
          </CardContent>
        </Card>
      );
    case "pdf":
      return <iframe src={src} title={item.name} className="h-[80vh] w-full rounded-lg border bg-white" />;
    case "text":
      return <TextContent item={item} src={src} />;
    default:
      return <Other item={item} />;
  }
}

function Viewer() {
  const path = usePath();
  const id = path?.split("/")[1] ?? "";
  const initial = usePageData<PublicItem>();
  const fromPage = initial?.id === id ? initial : null;
  const fetched = useQuery({ queryKey: ["public", id], queryFn: () => api<PublicItem>(`/public/items/${id}`), enabled: !!id && !fromPage, retry: false });
  const [unlocked, setItem] = useState<PublicItem | null>(null);
  const item = unlocked ?? fromPage ?? fetched.data ?? null;
  const missing = fetched.isError;
  const [reveal, setReveal] = useState(false);
  const [editing, setEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  if (missing)
    return (
      <div className="py-24 text-center">
        <p className="text-lg font-medium">Not found</p>
        <p className="text-sm text-muted-foreground">This file was deleted or never existed.</p>
      </div>
    );
  if (!item) return <Skeleton className="mx-auto h-[60vh] w-full rounded-lg" />;
  if (item.locked && !item.unlocked) return <Unlock item={item} onUnlocked={setItem} />;
  if (item.burn && !reveal && !item.owner) return <Burn onReveal={() => setReveal(true)} />;

  const facts = [bytes(item.size), item.width && item.height ? `${item.width}×${item.height}` : null, item.duration ? duration(item.duration) : null, `${item.views ?? 0} views`].filter(Boolean);
  return (
    <div className="flex flex-col gap-4">
      <Media item={item} reveal={reveal} />
      <div className="flex flex-wrap items-center gap-4 border-t pt-4">
        <span className="hidden size-9 shrink-0 items-center justify-center rounded-md border text-muted-foreground sm:flex">
          <ItemIcon type={item.type} className="size-4" />
        </span>
        <div className="min-w-0 flex-1">
          <h1 className="truncate font-semibold">{item.name}</h1>
          <p className="flex flex-wrap items-center gap-x-1.5 text-sm text-muted-foreground">
            {item.uploader && (
              <>
                <Avatar className="size-4">
                  {item.uploader.avatar_url && <AvatarImage src={item.uploader.avatar_url} alt="" />}
                  <AvatarFallback className="text-[9px]">{item.uploader.name.slice(0, 1).toUpperCase()}</AvatarFallback>
                </Avatar>
                <span>{item.uploader.name}</span>
                <span aria-hidden>·</span>
              </>
            )}
            <span title={date(item.created_at)}>{ago(item.created_at)}</span>
            {[...facts, item.expires_at ? `deleted ${ago(item.expires_at)}` : null].filter(Boolean).map((f) => (
              <span key={String(f)} className="contents">
                <span aria-hidden>·</span>
                <span>{f}</span>
              </span>
            ))}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => copy(item.url ?? window.location.href)}>
            <Copy /> Copy link
          </Button>
          {item.url && <QrButton url={item.url} name={item.name} />}
          {!item.burn && (
            <Button variant="outline" size="sm" render={<a href={item.raw_url} target="_blank" rel="noreferrer" />}>
              <ExternalLink /> Raw
            </Button>
          )}
          {!item.burn && (
            <Button variant="outline" size="sm" render={<a href={item.download_url} />}>
              <Download /> Download
            </Button>
          )}
          {item.owner && (
            <>
              <Button variant="outline" size="sm" onClick={() => setEditing(true)}>
                <Pencil /> Edit
              </Button>
              <Button
                variant="destructive"
                size="sm"
                onClick={async () => {
                  if (!confirmDelete) return setConfirmDelete(true);
                  await api(`/items/${item.id}`, { method: "DELETE" });
                  toast.success("Deleted");
                  window.location.replace("/files");
                }}
              >
                <Trash2 /> {confirmDelete ? "Really?" : "Delete"}
              </Button>
            </>
          )}
        </div>
      </div>
      {item.owner && editing && (
        <ItemDialog
          item={item as Item}
          onClose={() => setEditing(false)}
          onChange={(it) => setItem({ ...item, ...it, owner: true, unlocked: true, content: item.content })}
          onDeleted={() => window.location.replace("/files")}
        />
      )}
    </div>
  );
}

export default function ViewPage() {
  return (
    <div className="flex min-h-svh flex-col">
      <PublicHeader />
      <main className="mx-auto w-full max-w-5xl flex-1 px-4 py-6">
        <Viewer />
      </main>
      <Footer />
    </div>
  );
}
