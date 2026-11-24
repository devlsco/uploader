"use client";

import { Check, Copy, ExternalLink, RotateCcw, Upload, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { ItemIcon } from "@/components/app/item-icon";
import { Button } from "@/components/ui/button";
import type { UploadOptions } from "@/lib/api";
import { copy } from "@/lib/copy";
import { bytes } from "@/lib/format";
import { type Job, queue, useJobs } from "@/lib/upload";
import { cn } from "cn";

function typeOf(f: File) {
  const t = f.type.split("/")[0];
  return t === "image" || t === "video" || t === "audio" || t === "text" ? t : f.type === "application/pdf" ? "pdf" : "other";
}

function JobRow({ job }: { job: Job }) {
  const pct = job.file.size ? Math.round((job.loaded / job.file.size) * 100) : 0;
  const speed = job.speed ?? 0;
  return (
    <div className="flex items-center gap-3 rounded-lg border bg-card p-2">
      <div className="flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
        {job.preview ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img src={job.preview} alt="" className="size-full object-cover" />
        ) : (
          <ItemIcon type={typeOf(job.file)} className="size-5 text-muted-foreground" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span className="truncate text-sm font-medium">{job.file.name}</span>
          <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
            {job.status === "uploading" ? `${pct}%${speed ? ` · ${bytes(speed)}/s` : ""}` : bytes(job.file.size)}
          </span>
        </div>
        {job.status === "done" && job.item ? (
          <button type="button" onClick={() => copy(job.item!.url)} className="block max-w-full truncate text-left text-xs text-muted-foreground hover:text-foreground">
            {job.item.url}
          </button>
        ) : job.status === "error" ? (
          <p className="truncate text-xs text-destructive">{job.error}</p>
        ) : (
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-muted">
            <div className={cn("h-full rounded-full bg-foreground transition-[width] duration-200", job.status === "queued" && "w-0")} style={{ width: `${pct}%` }} />
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        {job.status === "done" && job.item && (
          <>
            <Button variant="ghost" size="icon-sm" aria-label="Copy link" onClick={() => copy(job.item!.url)}>
              <Copy />
            </Button>
            <Button variant="ghost" size="icon-sm" aria-label="Open" render={<a href={job.item.url} target="_blank" rel="noreferrer" />}>
              <ExternalLink />
            </Button>
          </>
        )}
        {job.status === "error" && (
          <Button variant="ghost" size="icon-sm" aria-label="Retry" onClick={() => queue.retry(job.key)}>
            <RotateCcw />
          </Button>
        )}
        <Button variant="ghost" size="icon-sm" aria-label={job.status === "uploading" ? "Cancel" : "Remove"} onClick={() => queue.remove(job.key)}>
          <X />
        </Button>
      </div>
    </div>
  );
}

/** Drop, choose or paste files; the whole window takes drops while this is shown. */
export function Dropzone({ options, maxSize, chunkSize }: { options: UploadOptions; maxSize: number; chunkSize: number }) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const jobs = useJobs();
  const opts = useRef(options);
  useEffect(() => {
    opts.current = options;
  }, [options]);
  useEffect(() => {
    queue.chunkSize = chunkSize;
  }, [chunkSize]);

  const add = (files: File[]) => {
    const ok = files.filter((f) => {
      if (f.size > maxSize) {
        toast.error(`${f.name} is larger than ${bytes(maxSize)}`);
        return false;
      }
      return true;
    });
    if (ok.length) queue.add(ok, opts.current);
  };
  const addRef = useRef(add);
  useEffect(() => {
    addRef.current = add;
  });

  useEffect(() => {
    let depth = 0;
    const hasFiles = (e: DragEvent) => !!e.dataTransfer?.types.includes("Files");
    const enter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      depth++;
      setOver(true);
    };
    const leave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) setOver(false);
    };
    const overFn = (e: DragEvent) => {
      if (hasFiles(e)) e.preventDefault();
    };
    const drop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth = 0;
      setOver(false);
      addRef.current(Array.from(e.dataTransfer?.files ?? []));
    };
    const paste = (e: ClipboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
      const files = Array.from(e.clipboardData?.files ?? []);
      if (files.length) {
        e.preventDefault();
        addRef.current(files.map((f) => (f.name === "image.png" ? new File([f], `screenshot-${new Date().toISOString().slice(0, 19).replace(/[T:]/g, "-")}.png`, { type: f.type }) : f)));
      }
    };
    window.addEventListener("dragenter", enter);
    window.addEventListener("dragleave", leave);
    window.addEventListener("dragover", overFn);
    window.addEventListener("drop", drop);
    window.addEventListener("paste", paste);
    return () => {
      window.removeEventListener("dragenter", enter);
      window.removeEventListener("dragleave", leave);
      window.removeEventListener("dragover", overFn);
      window.removeEventListener("drop", drop);
      window.removeEventListener("paste", paste);
    };
  }, []);

  const done = jobs.filter((j) => j.status === "done" || j.status === "error").length;
  return (
    <div className="flex flex-col gap-3" data-tour="dropzone">
      <button
        type="button"
        onClick={() => input.current?.click()}
        className={cn(
          "group relative flex min-h-64 flex-col items-center justify-center gap-4 rounded-lg border border-dashed bg-surface p-10 text-center hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none",
          over && "drop-active border-transparent bg-muted",
        )}
      >
        <span className="flex size-10 items-center justify-center rounded-md border bg-background shadow-xs">
          {over ? <Check className="size-4" /> : <Upload className="size-4" />}
        </span>
        <span className="flex flex-col gap-1">
          <span className="text-sm font-medium">{over ? "Drop to upload" : "Drop files here, or click to choose"}</span>
          <span className="text-xs text-muted-foreground">
            or paste with <kbd className="rounded-md border bg-muted px-1.5 py-0.5 font-mono text-xs">Ctrl</kbd> <kbd className="rounded-md border bg-muted px-1.5 py-0.5 font-mono text-xs">V</kbd>
          </span>
        </span>
        <span className="text-xs text-muted-foreground">Up to {bytes(maxSize)} per file</span>
      </button>
      <input
        ref={input}
        type="file"
        multiple
        hidden
        onChange={(e) => {
          add(Array.from(e.target.files ?? []));
          e.target.value = "";
        }}
      />
      {jobs.length > 0 && (
        <div className="flex flex-col gap-2">
          {done > 1 && (
            <div className="flex justify-end">
              <Button variant="ghost" size="sm" onClick={() => queue.clearDone()}>
                Clear list
              </Button>
            </div>
          )}
          {jobs.map((j) => (
            <JobRow key={j.key} job={j} />
          ))}
        </div>
      )}
      {over && <div className="pointer-events-none fixed inset-0 z-30 bg-background/40" />}
    </div>
  );
}
