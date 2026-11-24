"use client";

import { useMutation } from "@tanstack/react-query";
import { Check, Copy, ExternalLink } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { OptionSelect } from "@/components/app/option-select";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { api, type Item, type UploadOptions } from "@/lib/api";
import { copy } from "@/lib/copy";
import { LANGUAGES } from "@/lib/languages";

export function Result({ item, onReset, label }: { item: Item; onReset: () => void; label: string }) {
  return (
    <div className="flex flex-col items-center gap-4 rounded-lg border bg-card p-8 text-center">
      <span className="flex size-10 items-center justify-center rounded-md border bg-background">
        <Check className="size-4" />
      </span>
      <p className="text-sm text-muted-foreground">{label}</p>
      <button type="button" onClick={() => copy(item.url)} className="max-w-full truncate rounded-md border bg-muted px-3 py-1.5 font-mono text-sm hover:bg-muted/70">
        {item.url}
      </button>
      <div className="flex gap-2">
        <Button onClick={() => copy(item.url)}>
          <Copy /> Copy
        </Button>
        <Button variant="outline" render={<a href={item.url} target="_blank" rel="noreferrer" />}>
          <ExternalLink /> Open
        </Button>
        <Button variant="ghost" onClick={onReset}>
          New
        </Button>
      </div>
    </div>
  );
}

export function PasteForm({ options }: { options: UploadOptions }) {
  const [content, setContent] = useState("");
  const [title, setTitle] = useState("");
  const [language, setLanguage] = useState("auto");
  const [done, setDone] = useState<Item | null>(null);
  const create = useMutation({
    mutationFn: () =>
      api<Item>("/paste", {
        body: { content, title: title || undefined, language, ...options, expires: options.expires === "never" ? undefined : options.expires },
      }),
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
        label="Paste created"
        onReset={() => {
          setDone(null);
          setContent("");
          setTitle("");
        }}
      />
    );
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (content.trim()) create.mutate();
      }}
    >
      <div className="grid grid-cols-[1fr_auto] gap-2">
        <Input placeholder="Title (optional)" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} />
        <div className="w-40">
          <OptionSelect value={language} options={LANGUAGES} onChange={setLanguage} />
        </div>
      </div>
      <Textarea
        autoFocus
        value={content}
        onChange={(e) => setContent(e.target.value)}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && content.trim()) create.mutate();
          if (e.key === "Tab" && !e.shiftKey) {
            e.preventDefault();
            const t = e.currentTarget;
            const [a, b] = [t.selectionStart, t.selectionEnd];
            setContent(content.slice(0, a) + "  " + content.slice(b));
            requestAnimationFrame(() => t.setSelectionRange(a + 2, a + 2));
          }
        }}
        placeholder="Paste or write text…"
        spellCheck={false}
        className="min-h-[45vh] resize-y bg-card font-mono text-sm"
      />
      <div className="flex items-center justify-end gap-3">
        <span className="hidden text-xs text-muted-foreground sm:inline">Ctrl + Enter</span>
        <Button type="submit" disabled={!content.trim() || create.isPending}>
          Create paste
        </Button>
      </div>
    </form>
  );
}
