"use client";

import { useMutation } from "@tanstack/react-query";
import { Link2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, type Item, type UploadOptions } from "@/lib/api";

import { Result } from "./paste-form";

export function LinkForm({ base, options }: { base: string; options: UploadOptions }) {
  const [url, setUrl] = useState("");
  const [slug, setSlug] = useState("");
  const [done, setDone] = useState<Item | null>(null);
  const create = useMutation({
    mutationFn: () => api<Item>("/links", { body: { url, slug: slug || undefined, ...options, album: undefined, expires: options.expires === "never" ? undefined : options.expires } }),
    onSuccess: (item) => {
      setDone(item);
      void navigator.clipboard?.writeText(item.url).then(() => toast.success("Link copied"), () => {});
    },
    onError: (e) => toast.error(e.message),
  });
  if (done)
    return (
      <Result
        item={done}
        label="Short link created"
        onReset={() => {
          setDone(null);
          setUrl("");
          setSlug("");
        }}
      />
    );
  const host = base.replace(/^https?:\/\//, "");
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (url.trim()) create.mutate();
      }}
    >
      <div className="relative">
        <Link2 className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input autoFocus type="url" required placeholder="https://…" value={url} onChange={(e) => setUrl(e.target.value)} className="h-10 bg-card pl-9" />
      </div>
      <div className="flex items-center overflow-hidden rounded-md border bg-card">
        <span className="flex h-8 shrink-0 items-center border-r bg-muted px-2.5 text-sm text-muted-foreground">{host}/</span>
        <input
          className="h-8 min-w-0 flex-1 bg-transparent px-2.5 text-sm outline-none"
          placeholder="custom-name (optional)"
          value={slug}
          onChange={(e) => setSlug(e.target.value.replace(/[^a-zA-Z0-9_-]/g, ""))}
          maxLength={48}
        />
      </div>
      <div className="flex justify-end">
        <Button type="submit" disabled={!url.trim() || create.isPending}>
          Shorten
        </Button>
      </div>
    </form>
  );
}
