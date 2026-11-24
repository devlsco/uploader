"use client";

import { Check, Copy, WrapText } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { languageOf } from "@/lib/languages";
import { cn } from "cn";

/** Highlighting only up to this size; bigger text is shown plain (stays fast). */
const HIGHLIGHT_MAX = 400_000;

export function CodeView({ code: raw, name, language, mime }: { code: string; name: string; language?: string | null; mime?: string }) {
  const code = raw.endsWith("\n") ? raw.slice(0, -1) : raw;
  const [html, setHtml] = useState<string | null>(null);
  const [detected, setDetected] = useState<string | null>(null);
  const [wrap, setWrap] = useState(false);
  const [copied, setCopied] = useState(false);
  const lang = language ?? languageOf(name, mime);
  const lines = useMemo(() => code.split("\n").length, [code]);

  useEffect(() => {
    if (code.length > HIGHLIGHT_MAX || lang === "plaintext") return;
    let cancelled = false;
    void import("highlight.js/lib/common").then(({ default: hljs }) => {
      if (cancelled) return;
      const run = () => {
        const r = lang && hljs.getLanguage(lang) ? hljs.highlight(code, { language: lang, ignoreIllegals: true }) : code.length < 100_000 ? hljs.highlightAuto(code) : null;
        if (!cancelled && r) {
          setHtml(r.value);
          setDetected(r.language ?? null);
        }
      };
      if ("requestIdleCallback" in window) window.requestIdleCallback(run, { timeout: 500 });
      else setTimeout(run, 0);
    });
    return () => {
      cancelled = true;
    };
  }, [code, lang]);

  const gutter = useMemo(() => Array.from({ length: lines }, (_, i) => i + 1).join("\n"), [lines]);
  return (
    <div className="overflow-hidden rounded-lg border bg-card">
      <div className="flex items-center justify-between gap-2 border-b px-3 py-1.5">
        <span className="text-xs text-muted-foreground">
          {lines.toLocaleString()} lines{(lang ?? detected) ? ` · ${lang ?? detected}` : ""}
        </span>
        <div className="flex gap-1">
          <Button variant={wrap ? "secondary" : "ghost"} size="icon-sm" aria-label="Wrap lines" onClick={() => setWrap(!wrap)}>
            <WrapText />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Copy text"
            onClick={async () => {
              await navigator.clipboard.writeText(code);
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            }}
          >
            {copied ? <Check /> : <Copy />}
          </Button>
        </div>
      </div>
      <div className="flex max-h-[78vh] overflow-auto font-mono text-[13px] leading-6">
        {!wrap && (
          <pre aria-hidden className="sticky left-0 shrink-0 bg-card py-3 pr-3 pl-4 text-right text-muted-foreground/60 select-none">
            {gutter}
          </pre>
        )}
        <pre className={cn("hljs min-w-0 flex-1 py-3 pr-4", wrap ? "pl-4 break-words whitespace-pre-wrap" : "pl-1")}>
          {html ? <code dangerouslySetInnerHTML={{ __html: html }} /> : <code>{code}</code>}
        </pre>
      </div>
    </div>
  );
}
