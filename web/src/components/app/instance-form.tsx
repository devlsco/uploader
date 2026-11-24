"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, ExternalLink } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type Settings } from "@/lib/api";
import { copy } from "@/lib/copy";
import { cn } from "cn";

import { OptionSelect } from "./option-select";

function Section({ title, description, children, compact }: { title: string; description?: string; children: React.ReactNode; compact?: boolean }) {
  return (
    <section className={cn("grid gap-4 border-b pb-6 text-left last:border-b-0 last:pb-0", !compact && "md:grid-cols-[14rem_1fr]")}>
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        {description && <p className="mt-1 text-xs text-muted-foreground">{description}</p>}
      </div>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="grid items-center gap-1.5 sm:grid-cols-[10rem_1fr]">
      <div>
        <span className="text-sm">{label}</span>
        {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      </div>
      {children}
    </div>
  );
}

function Unit({ unit, children }: { unit: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-2">
      {children}
      <span className="shrink-0 text-sm text-muted-foreground">{unit}</span>
    </div>
  );
}

/** All instance settings; used in the setup (step 3) and the admin panel. */
export function InstanceForm({ submitLabel = "Save", onSaved, compact }: { submitLabel?: string; onSaved?: () => void; compact?: boolean }) {
  const settings = useQuery({ queryKey: ["admin", "settings"], queryFn: () => api<Settings>("/admin/settings") });
  if (!settings.data) return <Skeleton className="h-96 w-full" />;
  return <Form initial={settings.data} submitLabel={submitLabel} onSaved={onSaved} compact={compact} />;
}

function Form({ initial, submitLabel, onSaved, compact }: { initial: Settings; submitLabel: string; onSaved?: () => void; compact?: boolean }) {
  const qc = useQueryClient();
  const [s, setS] = useState<Settings>(initial);
  const set = (p: Partial<Settings>) => setS({ ...s, ...p });
  const save = useMutation({
    mutationFn: (v: Settings) => api<Settings>("/admin/settings", { method: "PUT", body: v }),
    onSuccess: (v) => {
      qc.setQueryData(["admin", "settings"], v);
      setS(v);
      void qc.invalidateQueries({ queryKey: ["instance"] });
      toast.success("Saved");
      onSaved?.();
    },
    onError: (e) => toast.error(e.message),
  });
  const twitchReady = !!s.twitch_client_id && (!!s.twitch_client_secret || !!s.twitch_secret_set);
  return (
    <form
      className="flex flex-col gap-6"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate(s);
      }}
    >
      <Section compact={compact} title="Accounts" description="Who can sign up. Twitch login is optional.">
        <Row label="Sign up">
          <OptionSelect
            value={s.registration}
            options={[
              { value: "closed", label: "Closed", hint: "Only admins add accounts" },
              { value: "invite", label: "Invite only", hint: "With an invite link" },
              { value: "open", label: "Open", hint: "Anyone can sign up" },
            ]}
            onChange={(v) => set({ registration: v as Settings["registration"] })}
          />
        </Row>
        <div className="flex flex-col gap-3 rounded-lg border bg-surface p-4">
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-medium">Twitch login</span>
            <a href="https://dev.twitch.tv/console/apps/create" target="_blank" rel="noreferrer" className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
              Create a Twitch app <ExternalLink className="size-3" />
            </a>
          </div>
          {s.twitch_redirect_url && (
            <div className="flex items-center gap-2">
              <span className="w-24 shrink-0 text-xs text-muted-foreground">Redirect URL</span>
              <code className="min-w-0 flex-1 truncate rounded-md bg-background px-2 py-1.5 text-xs">{s.twitch_redirect_url}</code>
              <Button type="button" variant="ghost" size="icon-sm" aria-label="Copy redirect URL" onClick={() => copy(s.twitch_redirect_url!, "Redirect URL")}>
                <Copy />
              </Button>
            </div>
          )}
          <Input placeholder="Client ID" value={s.twitch_client_id} onChange={(e) => set({ twitch_client_id: e.target.value })} autoComplete="off" spellCheck={false} className="font-mono" />
          <Input
            type="password"
            placeholder={s.twitch_secret_set ? "Client secret (saved, type to replace)" : "Client secret"}
            value={s.twitch_client_secret}
            onChange={(e) => set({ twitch_client_secret: e.target.value })}
            autoComplete="new-password"
            className="font-mono"
          />
          <p className="text-xs text-muted-foreground">
            {twitchReady ? "People can log in with Twitch or a password." : s.twitch_from_env ? "Using the Twitch app from the .env file." : "Optional. Passwords always work."}
          </p>
        </div>
      </Section>

      <Section compact={compact} title="Uploads" description="Limits for files.">
        <Row label="Max file size">
          <Unit unit="MB">
            <Input type="number" min={1} value={s.max_upload_mb} onChange={(e) => set({ max_upload_mb: Number(e.target.value) })} />
          </Unit>
        </Row>
        <Row label="Storage per user" hint="0 = unlimited">
          <Unit unit="MB">
            <Input type="number" min={0} value={s.default_quota_mb} onChange={(e) => set({ default_quota_mb: Number(e.target.value) })} />
          </Unit>
        </Row>
      </Section>

      <div className={cn("flex", compact ? "" : "justify-end")}>
        <Button type="submit" className={compact ? "w-full" : "min-w-28"} disabled={save.isPending}>
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
