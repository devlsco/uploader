import { useSyncExternalStore } from "react";

import type { Item, UploadOptions } from "./api";

export type Job = {
  key: string;
  file: File;
  preview: string | null;
  loaded: number;
  status: "queued" | "uploading" | "done" | "error";
  item?: Item;
  error?: string;
  started?: number;
  /** bytes per second */
  speed?: number;
  abort?: () => void;
};

type Result = { status: number; data: Record<string, unknown> };

function send(method: string, url: string, body: XMLHttpRequestBodyInit, headers: Record<string, string>, onProgress: (n: number) => void, setAbort: (f: () => void) => void): Promise<Result> {
  return new Promise((resolve, reject) => {
    const x = new XMLHttpRequest();
    x.open(method, url);
    for (const [k, v] of Object.entries(headers)) x.setRequestHeader(k, v);
    x.upload.onprogress = (e) => onProgress(e.loaded);
    x.onload = () => {
      let data: Record<string, unknown> = {};
      try {
        data = JSON.parse(x.responseText || "{}");
      } catch {
        data = {};
      }
      resolve({ status: x.status, data });
    };
    x.onerror = () => reject(new Error("Network error"));
    x.onabort = () => reject(new Error("Cancelled"));
    setAbort(() => x.abort());
    x.send(body);
  });
}

function fail(r: Result): never {
  throw new Error(typeof r.data.error === "string" ? r.data.error : r.status === 413 ? "File is too large" : `Upload failed (${r.status})`);
}

function query(o: UploadOptions): string {
  const p = new URLSearchParams();
  if (o.expires && o.expires !== "never") p.set("expires", o.expires);
  if (o.burn) p.set("burn", "1");
  if (o.max_views) p.set("max_views", String(o.max_views));
  if (o.album) p.set("album", o.album);
  const s = p.toString();
  return s ? `?${s}` : "";
}

const PARALLEL = 3;

class Queue {
  jobs: Job[] = [];
  private listeners = new Set<() => void>();
  private opts = new Map<string, UploadOptions>();
  onDone: ((item: Item) => void) | null = null;
  chunkSize = 32 * 1024 * 1024;

  subscribe = (l: () => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };
  snapshot = () => this.jobs;

  private emit() {
    this.jobs = [...this.jobs];
    this.listeners.forEach((l) => l());
  }

  private patch(key: string, p: Partial<Job>) {
    const i = this.jobs.findIndex((j) => j.key === key);
    if (i >= 0) {
      this.jobs[i] = { ...this.jobs[i], ...p };
      this.emit();
    }
  }

  add(files: File[], opts: UploadOptions) {
    for (const file of files) {
      const key = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      const preview = file.type.startsWith("image/") && file.size < 30 * 1024 * 1024 ? URL.createObjectURL(file) : null;
      this.opts.set(key, opts);
      this.jobs = [{ key, file, preview, loaded: 0, status: "queued" }, ...this.jobs];
    }
    this.emit();
    this.pump();
  }

  remove(key: string) {
    const j = this.jobs.find((j) => j.key === key);
    j?.abort?.();
    if (j?.preview) URL.revokeObjectURL(j.preview);
    this.jobs = this.jobs.filter((x) => x.key !== key);
    this.emit();
    this.pump();
  }

  clearDone() {
    for (const j of this.jobs) if (j.status === "done" && j.preview) URL.revokeObjectURL(j.preview);
    this.jobs = this.jobs.filter((j) => j.status !== "done" && j.status !== "error");
    this.emit();
  }

  retry(key: string) {
    this.patch(key, { status: "queued", error: undefined, loaded: 0 });
    this.pump();
  }

  private pump() {
    const running = this.jobs.filter((j) => j.status === "uploading").length;
    const next = this.jobs.filter((j) => j.status === "queued").reverse().slice(0, Math.max(0, PARALLEL - running));
    for (const j of next) void this.run(j);
  }

  private async run(job: Job) {
    const opts = this.opts.get(job.key) ?? {};
    this.patch(job.key, { status: "uploading", started: Date.now(), loaded: 0 });
    const setAbort = (f: () => void) => this.patch(job.key, { abort: f });
    const started = Date.now();
    const progress = (loaded: number) => {
      const secs = (Date.now() - started) / 1000;
      this.patch(job.key, { loaded: Math.min(loaded, job.file.size), speed: secs > 0.5 ? loaded / secs : undefined });
    };
    const headers: Record<string, string> = {};
    if (opts.password) headers["X-Password"] = opts.password;
    try {
      let item: Item;
      if (job.file.size <= this.chunkSize) {
        const fd = new FormData();
        fd.append("file", job.file, job.file.name);
        const r = await send("POST", `/api/upload${query(opts)}`, fd, headers, progress, setAbort);
        if (r.status !== 200) fail(r);
        item = r.data as unknown as Item;
      } else {
        const start = await fetch("/api/uploads", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ name: job.file.name, size: job.file.size, ...opts, expires: opts.expires === "never" ? undefined : opts.expires }),
        });
        const s = await start.json();
        if (!start.ok) throw new Error(s.error ?? "Upload failed");
        const size: number = s.chunk_size;
        for (let off = 0; off < job.file.size; off += size) {
          const part = job.file.slice(off, off + size);
          let tries = 0;
          for (;;) {
            try {
              const r = await send("PUT", `/api/uploads/${s.id}?offset=${off}`, part, {}, (n) => progress(off + n), setAbort);
              if (r.status !== 200) fail(r);
              break;
            } catch (e) {
              if (++tries > 2 || (e as Error).message === "Cancelled") throw e;
              await new Promise((r) => setTimeout(r, 1000 * tries));
            }
          }
        }
        const done = await send("POST", `/api/uploads/${s.id}/complete`, "", {}, () => {}, setAbort);
        if (done.status !== 200) fail(done);
        item = done.data as unknown as Item;
      }
      this.patch(job.key, { status: "done", item, loaded: job.file.size, abort: undefined });
      this.onDone?.(item);
    } catch (e) {
      this.patch(job.key, { status: "error", error: (e as Error).message, abort: undefined });
    } finally {
      this.pump();
    }
  }
}

export const queue = new Queue();

export function useJobs(): Job[] {
  return useSyncExternalStore(queue.subscribe, queue.snapshot, () => []);
}
