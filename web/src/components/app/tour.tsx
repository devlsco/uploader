"use client";

import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, ArrowRight, Check, FolderOpen, Link2, Settings, ShieldCheck, Sparkles, Upload, X } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { api, type Instance } from "@/lib/api";
import { queue, useJobs } from "@/lib/upload";
import { cn } from "cn";

type Step = {
  id: string;
  /** the element with data-tour="…" in the spotlight; none = card in the middle */
  target?: string;
  icon: typeof Upload;
  title: string;
  text: React.ReactNode;
  /** waits until the visitor uploaded a file */
  upload?: boolean;
  /** room around the highlight (small for the navigation labels) */
  pad?: number;
};

const kbd = "rounded border bg-muted px-1 py-px font-mono text-[11px] text-foreground";

const STEPS: Step[] = [
  { id: "welcome", icon: Sparkles, title: "", text: "" },
  {
    id: "dropzone",
    target: "dropzone",
    icon: Upload,
    title: "Upload your first file",
    text: (
      <>
        Click the box to pick a file, drop one on it, or paste a screenshot with <kbd className={kbd}>Ctrl</kbd> <kbd className={kbd}>V</kbd>. Go ahead, try it!
      </>
    ),
    upload: true,
  },
  {
    id: "options",
    target: "options",
    icon: ShieldCheck,
    title: "You stay in control",
    text: "Let files delete themselves after a while, lock them with a password, or allow just one view.",
  },
  {
    id: "tabs",
    target: "tabs",
    icon: Link2,
    title: "Text and links, too",
    text: "Paste code and it gets colours. Shorten long links, with your own name if you like.",
  },
  {
    id: "files",
    target: "nav-files",
    pad: 6,
    icon: FolderOpen,
    title: "Everything in one place",
    text: "All your uploads with search and albums. Share, rename or delete them any time.",
  },
  {
    id: "settings",
    target: "nav-settings",
    pad: 6,
    icon: Settings,
    title: "Upload from your apps",
    text: "Connect ShareX or Chatterino in Settings, and your screenshots upload by themselves.",
  },
  { id: "done", icon: Check, title: "", text: "" },
];

/** Room around the highlighted element. */
const PAD = 8;
/** Space between the highlight and the card. */
const GAP = 14;

type Rect = { top: number; left: number; width: number; height: number };

/** The visible element for a step (desktop and phone navigation both carry the mark). */
function findTarget(name: string): HTMLElement | null {
  for (const el of document.querySelectorAll<HTMLElement>(`[data-tour="${name}"]`)) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) return el;
  }
  return null;
}

/** Follows the target's position (it moves when the page scrolls or the list grows). */
function useTargetRect(target: string | undefined): Rect | null {
  const [state, setState] = useState<{ target: string; rect: Rect } | null>(null);
  useEffect(() => {
    if (!target) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    findTarget(target)?.scrollIntoView({ block: "center", behavior: reduce ? "auto" : "smooth" });
    let last = "";
    let frame = 0;
    const tick = () => {
      const r = findTarget(target)?.getBoundingClientRect();
      const key = r ? `${Math.round(r.top)}|${Math.round(r.left)}|${Math.round(r.width)}|${Math.round(r.height)}` : "none";
      if (key !== last) {
        last = key;
        setState(r ? { target, rect: { top: r.top, left: r.left, width: r.width, height: r.height } } : null);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [target]);
  return target && state?.target === target ? state.rect : null;
}

/** A little burst of dots when the first upload worked. */
function Burst() {
  const colors = ["oklch(0.72 0.17 150)", "oklch(0.75 0.15 75)", "oklch(0.68 0.19 25)", "oklch(0.65 0.17 260)", "oklch(0.7 0.16 330)", "oklch(0.78 0.12 200)"];
  return (
    <span className="tour-burst pointer-events-none absolute inset-0" aria-hidden>
      {Array.from({ length: 12 }, (_, i) => (
        <span key={i} style={{ "--a": `${i * 30}deg`, background: colors[i % colors.length], animationDelay: `${(i % 3) * 40}ms` } as React.CSSProperties} />
      ))}
    </span>
  );
}

function Dots({ at }: { at: number }) {
  return (
    <div className="flex items-center gap-1" aria-label={`Step ${at} of ${STEPS.length - 2}`}>
      {STEPS.slice(1, -1).map((s, i) => (
        <span key={s.id} className={cn("h-1.5 rounded-full bg-muted-foreground/25 transition-all duration-300 motion-reduce:transition-none", i + 1 === at ? "w-4 bg-foreground" : "w-1.5", i + 1 < at && "bg-foreground/50")} />
      ))}
    </div>
  );
}

const doneKeys = () => new Set(queue.jobs.filter((j) => j.status === "done").map((j) => j.key));

/** Opens the file picker of the drop zone. */
const chooseFile = () => document.querySelector<HTMLInputElement>('[data-tour="dropzone"] input[type="file"]')?.click();

export function Tour({ name, onStep, onClose }: { name: string; onStep?: (id: string) => void; onClose: () => void }) {
  const qc = useQueryClient();
  const [at, setAt] = useState(0);
  const [before, setBefore] = useState<Set<string>>(() => doneKeys());
  const step = STEPS[at];
  const rect = useTargetRect(step.target);
  const jobs = useJobs();
  const uploaded = step.upload ? jobs.find((j) => j.status === "done" && j.item && !before.has(j.key)) : undefined;
  const uploading = step.upload && !uploaded && jobs.some((j) => (j.status === "uploading" || j.status === "queued") && !before.has(j.key));

  const go = (i: number) => {
    const next = STEPS[i];
    onStep?.(next.id);
    if (next.upload) setBefore(doneKeys());
    setAt(i);
  };

  const finish = useCallback(() => {
    qc.setQueryData<Instance>(["instance"], (old) => (old?.user ? { ...old, user: { ...old.user, onboarded: true } } : old));
    void api("/me", { method: "PATCH", body: { onboarded: true } }).catch(() => {});
    onClose();
  }, [qc, onClose]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") finish();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [finish]);

  const centered = !step.target || !rect;
  const vw = typeof window === "undefined" ? 1024 : window.innerWidth;
  const vh = typeof window === "undefined" ? 768 : window.innerHeight;
  const width = Math.min(360, vw - 32);

  let cardStyle: React.CSSProperties = {};
  const pad = step.pad ?? PAD;
  if (rect && !centered) {
    const left = Math.min(Math.max(rect.left + rect.width / 2 - width / 2, 16), vw - width - 16);
    const below = rect.top + rect.height + pad + GAP;
    const above = rect.top - pad - GAP;
    if (vh - below >= 210) cardStyle = { left, top: below, width };
    else if (above >= 210) cardStyle = { left, bottom: vh - above, width };
    else cardStyle = { left, bottom: 16, width };
  }

  const Icon = step.icon;
  const middle = at > 0 && at < STEPS.length - 1;
  const box = rect && {
    top: Math.max(3, rect.top - pad),
    left: Math.max(3, rect.left - pad),
    bottom: Math.min(vh - 3, rect.top + rect.height + pad),
    right: Math.min(vw - 3, rect.left + rect.width + pad),
  };

  return (
    <div className="pointer-events-none fixed inset-0 z-[60]" role="dialog" aria-modal="true" aria-label="Welcome tour">
      {centered ? (
        <div className="pointer-events-auto absolute inset-0 bg-black/55 duration-300 animate-in fade-in-0 supports-backdrop-filter:backdrop-blur-[2px]" />
      ) : (
        box && (
          <>
            {/* clicks outside the highlight do nothing; inside it the page works as usual */}
            <div className="pointer-events-auto absolute inset-x-0 top-0" style={{ height: box.top }} />
            <div className="pointer-events-auto absolute inset-x-0 bottom-0" style={{ top: box.bottom }} />
            <div className="pointer-events-auto absolute left-0" style={{ top: box.top, height: box.bottom - box.top, width: box.left }} />
            <div className="pointer-events-auto absolute right-0" style={{ top: box.top, height: box.bottom - box.top, left: box.right }} />
            <div
              className="pointer-events-none absolute rounded-xl shadow-[0_0_0_9999px_rgb(0_0_0/0.55)] ring-2 ring-white/80 transition-all duration-300 ease-out motion-reduce:transition-none"
              style={{ top: box.top, left: box.left, width: box.right - box.left, height: box.bottom - box.top }}
            >
              {step.upload && !uploaded && <span className="tour-ring absolute inset-0 rounded-xl" />}
            </div>
          </>
        )
      )}

      <div className={cn(centered ? "pointer-events-auto absolute inset-0 flex items-center justify-center p-4" : "pointer-events-auto absolute transition-[top,left,bottom] duration-300 ease-out motion-reduce:transition-none")} style={cardStyle}>
        <div
          key={step.id}
          className={cn(
            "relative w-full overflow-hidden rounded-xl bg-popover text-popover-foreground shadow-2xl ring-1 ring-foreground/10 duration-300 animate-in fade-in-0 slide-in-from-bottom-2",
            centered ? "max-w-sm p-6 text-center" : "p-4",
          )}
        >
          {at === 0 ? (
            <>
              <div className="mx-auto mb-4 flex size-12 items-center justify-center rounded-full bg-muted">
                <Sparkles className="size-5" />
              </div>
              <h2 className="text-lg font-semibold tracking-tight">Welcome, {name}!</h2>
              <p className="mt-2 text-sm text-muted-foreground">Your own place for screenshots, files, text and short links. Let&apos;s look around, it takes less than a minute.</p>
              <div className="mt-6 flex flex-col gap-2">
                <Button size="lg" autoFocus onClick={() => go(1)}>
                  Show me around <ArrowRight />
                </Button>
                <Button size="lg" variant="ghost" onClick={finish}>
                  Skip, I&apos;ll explore myself
                </Button>
              </div>
            </>
          ) : at === STEPS.length - 1 ? (
            <>
              <div className="relative mx-auto mb-4 flex size-12 items-center justify-center rounded-full bg-foreground text-background">
                <Check className="size-5" />
                <Burst />
              </div>
              <h2 className="text-lg font-semibold tracking-tight">You&apos;re all set!</h2>
              <p className="mt-2 text-sm text-muted-foreground">Upload, get a link, paste it anywhere. You can take this tour again in Settings.</p>
              <div className="mt-6 flex flex-col gap-2">
                <Button size="lg" autoFocus onClick={finish}>
                  Start uploading
                </Button>
                <Button size="lg" variant="outline" render={<Link href="/settings" />} onClick={finish}>
                  <Settings /> Connect ShareX or Chatterino
                </Button>
              </div>
            </>
          ) : (
            <>
              <div className="mb-3 flex items-center gap-3">
                <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted">
                  <Icon className="size-4" />
                </span>
                {middle && <Dots at={at} />}
                <Button variant="ghost" size="icon-sm" className="-mr-1 ml-auto" aria-label="Close the tour" onClick={finish}>
                  <X />
                </Button>
              </div>
              <h2 className="font-semibold">{step.title}</h2>
              <p className="mt-1 text-sm text-muted-foreground">{step.text}</p>

              {step.upload && (
                <div className={cn("relative mt-3 flex items-center gap-2.5 rounded-lg border px-3 py-2 text-sm transition-colors", uploaded && "border-emerald-500/30 bg-emerald-500/10")}>
                  {uploaded ? (
                    <>
                      <span className="relative flex size-5 shrink-0 items-center justify-center rounded-full bg-emerald-500 text-white">
                        <Check className="size-3" />
                        <Burst />
                      </span>
                      <span className="min-w-0">
                        <span className="block font-medium">Uploaded! The link is copied.</span>
                        <span className="block truncate text-xs text-muted-foreground">{uploaded.item?.url}</span>
                      </span>
                    </>
                  ) : (
                    <>
                      <span className="relative flex size-2.5 shrink-0">
                        <span className="absolute inline-flex size-full animate-ping rounded-full bg-foreground/40 motion-reduce:animate-none" />
                        <span className="relative inline-flex size-2.5 rounded-full bg-foreground/70" />
                      </span>
                      <span className="text-muted-foreground">{uploading ? "Uploading…" : "Waiting for your first upload…"}</span>
                    </>
                  )}
                </div>
              )}

              <div className="mt-4 flex items-center gap-2">
                {at > 1 && (
                  <Button variant="ghost" size="sm" onClick={() => go(at - 1)}>
                    <ArrowLeft /> Back
                  </Button>
                )}
                <div className="ml-auto flex items-center gap-2">
                  {step.upload && !uploaded ? (
                    <>
                      <Button variant="ghost" size="sm" onClick={() => go(at + 1)}>
                        Skip
                      </Button>
                      <Button size="sm" autoFocus onClick={chooseFile} disabled={uploading}>
                        <Upload /> Choose a file
                      </Button>
                    </>
                  ) : (
                    <Button size="sm" autoFocus onClick={() => go(at + 1)}>
                      {at === STEPS.length - 2 ? "Finish" : "Next"} <ArrowRight />
                    </Button>
                  )}
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
