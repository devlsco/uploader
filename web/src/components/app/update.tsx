"use client";

import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowUpCircle, Copy, RefreshCw } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { api, type Version } from "@/lib/api";
import { copy } from "@/lib/copy";

export function useVersion() {
  return useQuery({ queryKey: ["version"], queryFn: () => api<Version>("/admin/version"), staleTime: 3_600_000, retry: false });
}

const DOCKER = "docker compose pull && docker compose up -d";

/** Waits until the restarted server answers with the new version, then reloads. */
async function waitForRestart(version: string) {
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    try {
      const r = await fetch("/api/instance", { cache: "no-store" });
      if (r.ok && (await r.json()).version === version) return window.location.reload();
    } catch {
      /* still restarting */
    }
  }
  window.location.reload();
}

export function UpdateButton({ v, size = "sm" }: { v: Version; size?: "sm" | "default" }) {
  const [restarting, setRestarting] = useState(false);
  const run = useMutation({
    mutationFn: () => api<{ updated: boolean; version?: string }>("/admin/update", { method: "POST" }),
    onSuccess: (r) => {
      if (!r.updated || !r.version) return toast.info("Already up to date");
      setRestarting(true);
      void waitForRestart(r.version);
    },
    onError: (e) => toast.error(e.message, { duration: 10_000 }),
  });
  if (v.docker)
    return (
      <Button size={size} variant="outline" onClick={() => copy(DOCKER, "Command")}>
        <Copy /> Copy update command
      </Button>
    );
  return (
    <Button size={size} onClick={() => run.mutate()} disabled={run.isPending || restarting}>
      <RefreshCw className={run.isPending || restarting ? "animate-spin" : ""} />
      {restarting ? "Restarting…" : run.isPending ? "Downloading…" : "Update now"}
    </Button>
  );
}

/** Thin bar under the header for admins when a new version is out. */
export function UpdateBanner() {
  const { data } = useVersion();
  const [hidden, setHidden] = useState(false);
  if (!data?.update || hidden) return null;
  return (
    <div className="border-b bg-surface">
      <div className="mx-auto flex max-w-5xl flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2 text-sm">
        <ArrowUpCircle className="size-4" />
        <span className="font-medium">Version {data.latest} is available</span>
        <span className="text-muted-foreground">You have {data.current}</span>
        <div className="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={() => setHidden(true)}>
            Later
          </Button>
          <UpdateButton v={data} />
        </div>
      </div>
    </div>
  );
}
