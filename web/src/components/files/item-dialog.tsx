"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Copy, Download, ExternalLink, Lock, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { ItemIcon } from "@/components/app/item-icon";
import { QrButton } from "@/components/qr/qr-dialog";
import { OptionSelect } from "@/components/app/option-select";
import { useAlbums } from "@/components/app/upload-options";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { api, type Item } from "@/lib/api";
import { removeItems, updateItem } from "@/lib/cache";
import { copy } from "@/lib/copy";
import { EXPIRY, ago, bytes, date, duration } from "@/lib/format";

function LinkRow({ label, url }: { label: string; url: string }) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-20 shrink-0 text-xs text-muted-foreground">{label}</span>
      <button type="button" onClick={() => copy(url)} className="min-w-0 flex-1 truncate rounded-md bg-muted px-2 py-1 text-left font-mono text-xs hover:bg-muted/70">
        {url}
      </button>
      <Button variant="ghost" size="icon-xs" aria-label={`Copy ${label}`} onClick={() => copy(url)}>
        <Copy />
      </Button>
    </div>
  );
}

export function ItemDialog({ item, onClose, onChange, onDeleted }: { item: Item | null; onClose: () => void; onChange?: (item: Item) => void; onDeleted?: () => void }) {
  return (
    <Dialog open={!!item} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[90svh] overflow-y-auto sm:max-w-lg">{item && <Details key={item.id} item={item} onClose={onClose} onChange={onChange} onDeleted={onDeleted} />}</DialogContent>
    </Dialog>
  );
}

function Details({ item: initial, onClose, onChange, onDeleted }: { item: Item; onClose: () => void; onChange?: (item: Item) => void; onDeleted?: () => void }) {
  const qc = useQueryClient();
  const albums = useAlbums();
  const [item, setItem] = useState(initial);
  const [name, setName] = useState(initial.name);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState(false);
  const save = useMutation({
    mutationFn: (body: Record<string, unknown>) => api<Item>(`/items/${item.id}`, { method: "PATCH", body }),
    onSuccess: (it) => {
      setItem(it);
      updateItem(qc, it);
      if (it.album_id !== item.album_id) void qc.invalidateQueries({ queryKey: ["albums"] });
      onChange?.(it);
      toast.success("Saved");
    },
    onError: (e) => toast.error(e.message),
  });
  const del = useMutation({
    mutationFn: () => api(`/items/${item.id}`, { method: "DELETE" }),
    onSuccess: () => {
      removeItems(qc, [item.id]);
      toast.success("Deleted");
      onClose();
      onDeleted?.();
    },
    onError: (e) => toast.error(e.message),
  });
  const facts = [
    item.kind === "link" ? item.target : bytes(item.size),
    item.width && item.height ? `${item.width}×${item.height}` : null,
    item.duration ? duration(item.duration) : null,
    `${item.views} views`,
    ago(item.created_at),
  ].filter(Boolean);

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <DialogTitle className="sr-only">{item.name}</DialogTitle>
      <a href={item.url} target="_blank" rel="noreferrer" className="flex aspect-video items-center justify-center overflow-hidden rounded-lg bg-muted">
        {item.thumb_url ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img src={item.thumb_url} alt="" className="size-full object-contain" />
        ) : (
          <ItemIcon type={item.type} className="size-10 text-muted-foreground" />
        )}
      </a>
      <div className="flex flex-col gap-1">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => name.trim() && name !== item.name && save.mutate({ name })}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className="font-medium"
          aria-label="Name"
        />
        <p className="truncate px-1 text-xs text-muted-foreground">{facts.join(" · ")}</p>
        <div className="flex flex-wrap gap-1 px-1">
          {item.locked && <Badge variant="secondary">Password</Badge>}
          {item.burn && <Badge variant="secondary">View once</Badge>}
          {item.expires_at && <Badge variant="secondary" title={date(item.expires_at)}>Deleted {ago(item.expires_at)}</Badge>}
          {item.max_views && <Badge variant="secondary">Max {item.max_views} views</Badge>}
        </div>
      </div>

      <div className="flex flex-col gap-1.5">
        <LinkRow label="Page" url={item.url} />
        {item.kind !== "link" && <LinkRow label="Direct" url={item.raw_url} />}
        {item.deletion_url && <LinkRow label="Delete link" url={item.deletion_url} />}
      </div>

      <div className="grid grid-cols-2 gap-2">
        <OptionSelect
          value=""
          options={[{ value: "", label: item.expires_at ? `Expires ${ago(item.expires_at)}` : "Keep forever" }, ...EXPIRY.map((e) => ({ value: e.value, label: e.value === "never" ? "Keep forever" : `Delete after ${e.label}` }))]}
          onChange={(v) => v && save.mutate({ expires: v === "never" ? null : v })}
        />
        {item.kind !== "link" ? (
          <OptionSelect
            value={item.album_id ?? ""}
            options={[{ value: "", label: "No album" }, ...(albums.data ?? []).map((a) => ({ value: a.id, label: a.title }))]}
            onChange={(v) => save.mutate({ album_id: v || null })}
          />
        ) : (
          <Input defaultValue={item.target} onBlur={(e) => e.target.value !== item.target && save.mutate({ target: e.target.value })} aria-label="Target" />
        )}
        {item.locked ? (
          <Button variant="outline" onClick={() => save.mutate({ password: null })}>
            <Lock /> Remove password
          </Button>
        ) : (
          <form
            className="flex gap-1"
            onSubmit={(e) => {
              e.preventDefault();
              if (password) save.mutate({ password });
              setPassword("");
            }}
          >
            <Input type="password" autoComplete="new-password" placeholder="Set password" value={password} onChange={(e) => setPassword(e.target.value)} />
            {password && (
              <Button type="submit" size="sm">
                Set
              </Button>
            )}
          </form>
        )}
        <label className="flex h-8 cursor-pointer items-center justify-between rounded-md border px-2.5 text-sm">
          View once
          <Switch checked={item.burn} onCheckedChange={(c) => save.mutate({ burn: c })} />
        </label>
      </div>

      <div className="flex flex-wrap justify-between gap-2">
        <div className="flex gap-2">
          <Button variant="outline" size="sm" render={<a href={item.url} target="_blank" rel="noreferrer" />}>
            <ExternalLink /> Open
          </Button>
          <QrButton url={item.url} name={item.name} />
          {item.kind !== "link" && (
            <Button variant="outline" size="sm" render={<a href={item.download_url} />}>
              <Download /> Download
            </Button>
          )}
        </div>
        <Button variant="destructive" size="sm" disabled={del.isPending} onClick={() => (confirm ? del.mutate() : setConfirm(true))}>
          <Trash2 /> {confirm ? "Really delete?" : "Delete"}
        </Button>
      </div>
    </div>
  );
}
