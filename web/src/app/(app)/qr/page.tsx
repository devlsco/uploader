"use client";

import { useMutation } from "@tanstack/react-query";
import { Clock, Copy, Download, FileCode2, Link2, Wifi } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { toast } from "sonner";

import { PageHeader } from "@/components/app/app-shell";
import { OptionSelect } from "@/components/app/option-select";
import { QrImage } from "@/components/qr/qr-image";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { api, type Item } from "@/lib/api";
import { date } from "@/lib/format";
import { QR_DEFAULTS, type QrOptions, copyPng, downloadUrl, qrPng, qrSvg, wifiText } from "@/lib/qr";

const VALIDITY = [
  { value: "always", label: "Always valid", hint: "The QR code holds the content itself" },
  { value: "1h", label: "Valid for 1 hour" },
  { value: "1d", label: "Valid for 1 day" },
  { value: "7d", label: "Valid for 7 days" },
  { value: "30d", label: "Valid for 30 days" },
];

const isUrl = (s: string) => /^https?:\/\/\S+$/i.test(s.trim());

function QrGenerator() {
  const [mode, setMode] = useState<"text" | "wifi">("text");
  const [text, setText] = useState("");
  const [ssid, setSsid] = useState("");
  const [wifiPw, setWifiPw] = useState("");
  const [security, setSecurity] = useState<"WPA" | "WEP" | "nopass">("WPA");
  const [hidden, setHidden] = useState(false);
  const [validity, setValidity] = useState("always");
  const [maxScans, setMaxScans] = useState("");
  const [opts, setOpts] = useState<QrOptions>(QR_DEFAULTS);
  const [created, setCreated] = useState<{ source: string; item: Item } | null>(null);

  const timed = mode === "text" && validity !== "always";
  const content = mode === "wifi" ? (ssid ? wifiText(ssid, wifiPw, security, hidden) : "") : text.trim();
  const live = created && created.source === `${content}|${validity}|${maxScans}` ? created.item : null;
  const encoded = timed ? (live?.url ?? "") : content;

  const create = useMutation({
    mutationFn: () => {
      const common = { expires: validity, max_views: maxScans ? Number(maxScans) : undefined };
      return isUrl(content) ? api<Item>("/links", { body: { url: content, ...common } }) : api<Item>("/paste", { body: { content, title: "QR code", ...common } });
    },
    onSuccess: (item) => setCreated({ source: `${content}|${validity}|${maxScans}`, item }),
    onError: (e) => toast.error(e.message),
  });

  const name = `qr-${new Date().toISOString().slice(0, 10)}`;
  const set = (p: Partial<QrOptions>) => setOpts({ ...opts, ...p });

  return (
    <div className="grid gap-6 md:grid-cols-[1fr_20rem]">
      <div className="flex flex-col gap-4">
        <Tabs value={mode} onValueChange={(v) => setMode(v as "text" | "wifi")}>
          <TabsList>
            <TabsTrigger value="text">
              <Link2 /> Link or text
            </TabsTrigger>
            <TabsTrigger value="wifi">
              <Wifi /> WiFi
            </TabsTrigger>
          </TabsList>
        </Tabs>

        {mode === "text" ? (
          <Textarea autoFocus value={text} onChange={(e) => setText(e.target.value)} placeholder="https://… or any text" className="min-h-32" maxLength={2000} />
        ) : (
          <div className="grid gap-2 sm:grid-cols-2">
            <Input placeholder="Network name" value={ssid} onChange={(e) => setSsid(e.target.value)} />
            <Input placeholder="Password" value={wifiPw} onChange={(e) => setWifiPw(e.target.value)} disabled={security === "nopass"} />
            <OptionSelect
              value={security}
              options={[
                { value: "WPA", label: "WPA / WPA2 / WPA3" },
                { value: "WEP", label: "WEP" },
                { value: "nopass", label: "No password" },
              ]}
              onChange={(v) => setSecurity(v as typeof security)}
            />
            <Label className="flex h-8 items-center justify-between rounded-md border px-2.5 font-normal">
              Hidden network
              <Switch checked={hidden} onCheckedChange={setHidden} />
            </Label>
          </div>
        )}

        {mode === "text" && (
          <div className="grid gap-2 sm:grid-cols-[1fr_10rem]">
            <OptionSelect value={validity} options={VALIDITY} onChange={setValidity} />
            {timed && <Input type="number" min={1} placeholder="Max scans" value={maxScans} onChange={(e) => setMaxScans(e.target.value)} />}
          </div>
        )}
        {timed && (
          <div className="flex flex-wrap items-center gap-3">
            <Button onClick={() => create.mutate()} disabled={!content || create.isPending || !!live}>
              <Clock /> {live ? "Created" : "Create QR code"}
            </Button>
            {live && (
              <span className="text-sm text-muted-foreground">
                Works until {live.expires_at ? date(live.expires_at) : "—"}
                {live.max_views ? ` or ${live.max_views} scans` : ""} ·{" "}
                <Link href="/files" className="underline">
                  scans in Files
                </Link>
              </span>
            )}
          </div>
        )}

        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Label className="flex h-8 items-center justify-between gap-2 rounded-md border px-2.5 font-normal">
            Color
            <input type="color" value={opts.dark} onChange={(e) => set({ dark: e.target.value })} className="h-5 w-8 cursor-pointer bg-transparent" />
          </Label>
          <Label className="flex h-8 items-center justify-between gap-2 rounded-md border px-2.5 font-normal">
            Background
            <input type="color" value={opts.light} onChange={(e) => set({ light: e.target.value })} className="h-5 w-8 cursor-pointer bg-transparent" />
          </Label>
          <OptionSelect
            value={opts.ecc}
            options={[
              { value: "L", label: "Small", hint: "7% can be damaged" },
              { value: "M", label: "Normal", hint: "15% can be damaged" },
              { value: "Q", label: "Strong", hint: "25% can be damaged" },
              { value: "H", label: "Strongest", hint: "30% can be damaged" },
            ]}
            onChange={(v) => set({ ecc: v as QrOptions["ecc"] })}
          />
          <OptionSelect value={String(opts.size)} options={["256", "512", "1024", "2048"].map((s) => ({ value: s, label: `${s} px` }))} onChange={(v) => set({ size: Number(v) })} />
        </div>
      </div>

      <Card className="h-fit md:sticky md:top-20">
        <CardContent className="flex flex-col gap-3">
          {encoded ? (
            <QrImage text={encoded} options={opts} className="border" />
          ) : (
            <div className="flex aspect-square w-full flex-col items-center justify-center gap-2 rounded-lg border border-dashed text-sm text-muted-foreground">
              <FileCode2 className="size-6" />
              {timed && content ? "Press Create" : "Type something"}
            </div>
          )}
          <div className="grid grid-cols-3 gap-2">
            <Button variant="outline" size="sm" disabled={!encoded} onClick={async () => downloadUrl(await qrPng(encoded, opts), `${name}.png`)}>
              <Download /> PNG
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!encoded}
              onClick={async () => {
                const url = URL.createObjectURL(new Blob([await qrSvg(encoded, opts)], { type: "image/svg+xml" }));
                downloadUrl(url, `${name}.svg`);
                setTimeout(() => URL.revokeObjectURL(url), 1000);
              }}
            >
              <Download /> SVG
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!encoded}
              onClick={async () => {
                try {
                  await copyPng(await qrPng(encoded, opts));
                  toast.success("QR code copied");
                } catch {
                  toast.error("Copy failed");
                }
              }}
            >
              <Copy /> Copy
            </Button>
          </div>
          {timed && live && <p className="truncate text-center text-xs text-muted-foreground">{live.url}</p>}
        </CardContent>
      </Card>
    </div>
  );
}

export default function QrPage() {
  return (
    <>
      <PageHeader title="QR code" description="Made in your browser. Timed codes stop working after the time you pick." />
      <QrGenerator />
    </>
  );
}
