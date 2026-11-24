"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Download, KeyRound, Plus, Sparkles, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { PageHeader, useApp } from "@/components/app/app-shell";
import { OptionSelect } from "@/components/app/option-select";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useParam } from "@/hooks/use-param";
import { api, type Instance, type User } from "@/lib/api";
import { copy } from "@/lib/copy";
import { EXPIRY, ago } from "@/lib/format";

type Key = { id: number; name: string; prefix: string; created_at: number; last_used_at: number | null };
type NewKey = { id: number; name: string; key: string; upload_url: string };

function download(name: string, data: unknown) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function Field({ label, value, secret }: { label: string; value: string; secret?: boolean }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[9rem_1fr] sm:items-center">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div className="flex min-w-0 gap-1">
        <code className="min-w-0 flex-1 truncate rounded-md bg-muted px-2 py-1.5 text-xs">{secret ? `${value.slice(0, 14)}…` : value}</code>
        <Button variant="ghost" size="icon-sm" aria-label={`Copy ${label}`} onClick={() => copy(value, label)}>
          <Copy />
        </Button>
      </div>
    </div>
  );
}

function Account({ user }: { user: User }) {
  const qc = useQueryClient();
  const error = useParam("error");
  const [username, setUsername] = useState(user.username);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const save = useMutation({
    mutationFn: (body: Record<string, string>) => api("/me", { method: "PATCH", body }),
    onSuccess: () => {
      toast.success("Saved");
      setCurrent("");
      setNext("");
      void qc.invalidateQueries({ queryKey: ["instance"] });
    },
    onError: (e) => toast.error(e.message),
  });
  const unlink = useMutation({
    mutationFn: () => api("/me/twitch", { method: "DELETE" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["instance"] }),
    onError: (e) => toast.error(e.message),
  });
  const everywhere = useMutation({
    mutationFn: () => api("/me/sessions", { method: "DELETE" }),
    onSuccess: () => window.location.replace("/login"),
  });
  const instance = qc.getQueryData<Instance>(["instance"]);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Account</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error && <p className="text-sm text-destructive">{error}</p>}
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate({ username });
          }}
        >
          <Input value={username} onChange={(e) => setUsername(e.target.value)} aria-label="Username" />
          <Button type="submit" variant="outline" disabled={username === user.username}>
            Rename
          </Button>
        </form>
        <form
          className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate({ current_password: current, new_password: next });
          }}
        >
          {user.has_password && <Input type="password" autoComplete="current-password" placeholder="Current password" value={current} onChange={(e) => setCurrent(e.target.value)} />}
          <Input type="password" autoComplete="new-password" placeholder={user.has_password ? "New password" : "Set a password"} value={next} onChange={(e) => setNext(e.target.value)} />
          <Button type="submit" variant="outline" disabled={next.length < 8}>
            {user.has_password ? "Change" : "Set"}
          </Button>
        </form>
        <div className="flex flex-wrap gap-2">
          {instance?.twitch &&
            (user.twitch ? (
              <Button variant="outline" onClick={() => unlink.mutate()}>
                Unlink Twitch
              </Button>
            ) : (
              <Button variant="outline" render={<a href="/api/auth/twitch" />}>
                Link Twitch
              </Button>
            ))}
          <Button variant="outline" onClick={() => everywhere.mutate()}>
            Log out everywhere
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

const LINKS = [
  { value: "share", label: "Smart", hint: "Pictures as the file, the rest as the page" },
  { value: "page", label: "Page", hint: "Viewer page with preview" },
  { value: "direct", label: "Direct", hint: "The file itself, e.g. .png" },
];
const LINK_FIELD: Record<string, string> = { share: "share_link", page: "link", direct: "direct_link" };

function ShareXDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const qc = useQueryClient();
  const [kind, setKind] = useState("files");
  const [link, setLink] = useState("share");
  const [expires, setExpires] = useState("never");
  const [burn, setBurn] = useState(false);
  const host = new URL(instance.url).host;
  const make = useMutation({
    mutationFn: async () => {
      const k = await api<NewKey>("/keys", { body: { name: kind === "files" ? "ShareX" : "ShareX links" } });
      const base = { Version: "17.0.0", Name: `Uploader (${host})${kind === "links" ? " links" : ""}`, RequestMethod: "POST", Headers: { Authorization: `Bearer ${k.key}` }, ErrorMessage: "{json:error}" };
      const params: Record<string, string> = {};
      if (expires !== "never") params.expires = expires;
      if (burn) params.burn = "1";
      const cfg =
        kind === "files"
          ? {
              ...base,
              DestinationType: "ImageUploader, TextUploader, FileUploader",
              RequestURL: `${instance.url}/api/upload`,
              ...(Object.keys(params).length ? { Parameters: params } : {}),
              Body: "MultipartFormData",
              FileFormName: "file",
              URL: `{json:${LINK_FIELD[link]}}`,
              ThumbnailURL: "{json:thumbnail_link}",
              DeletionURL: "{json:deletion_link}",
            }
          : { ...base, DestinationType: "URLShortener", RequestURL: `${instance.url}/api/links`, Body: "JSON", Data: JSON.stringify({ url: "{input}", ...(expires !== "never" ? { expires } : {}) }), URL: "{json:link}", DeletionURL: "{json:deletion_link}" };
      download(`${host}${kind === "links" ? "-links" : ""}.sxcu`, cfg);
    },
    onSuccess: () => {
      toast.success("Open the downloaded file to add it to ShareX");
      void qc.invalidateQueries({ queryKey: ["keys"] });
      onClose();
    },
    onError: (e) => toast.error(e.message),
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>ShareX</DialogTitle>
          <DialogDescription>Download the file and open it, ShareX adds the uploader.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          <div className="grid gap-2">
            <Label>Uploads</Label>
            <OptionSelect
              value={kind}
              options={[
                { value: "files", label: "Files and screenshots" },
                { value: "links", label: "Short links" },
              ]}
              onChange={setKind}
            />
          </div>
          {kind === "files" && (
            <div className="grid gap-2">
              <Label>Link that is copied</Label>
              <OptionSelect value={link} options={LINKS} onChange={setLink} />
            </div>
          )}
          <div className="grid gap-2">
            <Label>Delete after</Label>
            <OptionSelect value={expires} options={EXPIRY.map((e) => ({ value: e.value, label: e.value === "never" ? "Keep forever" : e.label }))} onChange={setExpires} />
          </div>
          {kind === "files" && (
            <Label className="flex items-center justify-between font-normal">
              View once
              <Switch checked={burn} onCheckedChange={setBurn} />
            </Label>
          )}
        </div>
        <DialogFooter>
          <Button onClick={() => make.mutate()} disabled={make.isPending}>
            <Download /> Download .sxcu
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ChatterinoDialog({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient();
  const [link, setLink] = useState("share");
  const key = useQuery({
    queryKey: ["chatterino-key"],
    queryFn: async () => {
      const k = await api<NewKey>("/keys", { body: { name: "Chatterino" } });
      void qc.invalidateQueries({ queryKey: ["keys"] });
      return k;
    },
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Chatterino</DialogTitle>
          <DialogDescription>Settings → External tools → Image Uploader, then paste these.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-2">
          <Label>Link that is posted</Label>
          <OptionSelect value={link} options={LINKS} onChange={setLink} />
        </div>
        {key.data ? (
          <div className="flex flex-col gap-2">
            <Field label="Request URL" value={key.data.upload_url} />
            <Field label="Form field" value="file" />
            <Field label="Extra headers" value={`Authorization: Bearer ${key.data.key}`} secret />
            <Field label="Image link" value={`{${LINK_FIELD[link]}}`} />
            <Field label="Deletion link" value="{deletion_link}" />
          </div>
        ) : (
          <Skeleton className="h-40 w-full" />
        )}
        <DialogFooter>
          <Button onClick={onClose}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Apps({ instance }: { instance: Instance }) {
  const [open, setOpen] = useState<"sharex" | "chatterino" | null>(null);
  const apps = [
    { id: "sharex" as const, name: "ShareX", text: "Screenshots and files from Windows" },
    { id: "chatterino" as const, name: "Chatterino", text: "Images straight from the Twitch chat" },
  ];
  return (
    <Card>
      <CardHeader>
        <CardTitle>Apps</CardTitle>
        <CardDescription>Upload from other apps.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-3 sm:grid-cols-2">
        {apps.map((a) => (
          <button key={a.id} type="button" onClick={() => setOpen(a.id)} className="flex flex-col items-start gap-1 rounded-lg border p-4 text-left hover:border-foreground/25">
            <span className="text-sm font-medium">{a.name}</span>
            <span className="text-xs text-muted-foreground">{a.text}</span>
            <span className="mt-2 text-xs font-medium">Set up →</span>
          </button>
        ))}
      </CardContent>
      {open === "sharex" && <ShareXDialog instance={instance} onClose={() => setOpen(null)} />}
      {open === "chatterino" && <ChatterinoDialog onClose={() => setOpen(null)} />}
    </Card>
  );
}

function Keys() {
  const qc = useQueryClient();
  const keys = useQuery({ queryKey: ["keys"], queryFn: () => api<Key[]>("/keys") });
  const [name, setName] = useState("");
  const [created, setCreated] = useState<NewKey | null>(null);
  const create = useMutation({
    mutationFn: () => api<NewKey>("/keys", { body: { name } }),
    onSuccess: (k) => {
      setCreated(k);
      setName("");
      void qc.invalidateQueries({ queryKey: ["keys"] });
    },
    onError: (e) => toast.error(e.message),
  });
  const del = useMutation({
    mutationFn: (id: number) => api(`/keys/${id}`, { method: "DELETE" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["keys"] }),
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>API keys</CardTitle>
        <CardDescription>For your own scripts.</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) create.mutate();
          }}
        >
          <Input placeholder="Name, e.g. Laptop" value={name} onChange={(e) => setName(e.target.value)} maxLength={40} />
          <Button type="submit" disabled={!name.trim()}>
            <Plus /> Create
          </Button>
        </form>
        <pre className="overflow-x-auto rounded-md border bg-surface px-3 py-2 font-mono text-xs text-muted-foreground">
          curl -H &quot;Authorization: Bearer KEY&quot; -F file=@cat.png {typeof window !== "undefined" ? window.location.origin : ""}/api/upload
        </pre>
        {created && (
          <div className="flex flex-col gap-1 rounded-lg border border-primary/40 bg-primary/5 p-3">
            <span className="text-xs text-muted-foreground">Shown only once</span>
            <Field label={created.name} value={created.key} />
          </div>
        )}
        <div className="flex flex-col divide-y">
          {keys.data?.map((k) => (
            <div key={k.id} className="flex items-center gap-3 py-2">
              <KeyRound className="size-4 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm">{k.name}</p>
                <p className="text-xs text-muted-foreground">
                  {k.prefix}… · {k.last_used_at ? `used ${ago(k.last_used_at)}` : "never used"}
                </p>
              </div>
              <Button variant="ghost" size="icon-sm" aria-label="Delete key" onClick={() => del.mutate(k.id)}>
                <Trash2 />
              </Button>
            </div>
          ))}
          {keys.data?.length === 0 && <p className="py-2 text-sm text-muted-foreground">No keys yet</p>}
        </div>
      </CardContent>
    </Card>
  );
}

export default function SettingsPage() {
  const { user, instance } = useApp();
  return (
    <div>
      <PageHeader
        title="Settings"
        description="Connect your apps and manage your account."
        actions={
          <Button variant="outline" size="sm" onClick={() => window.location.assign("/?tour=1")}>
            <Sparkles /> Take the tour
          </Button>
        }
      />
      <div className="flex flex-col gap-6">
        <Apps instance={instance} />
        <Keys />
        <Account user={user} />
      </div>
    </div>
  );
}
