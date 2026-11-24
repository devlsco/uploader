"use client";

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, ShieldCheck, Terminal } from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Label } from "@/components/ui/label";
import { useInstance } from "@/hooks/use-instance";
import { useParam } from "@/hooks/use-param";
import { api } from "@/lib/api";
import { cn } from "cn";

import { InstanceForm } from "./instance-form";
import { Footer, Logo } from "./logo";

function TwitchIcon() {
  return (
    <svg viewBox="0 0 24 24" className="size-4" fill="currentColor" aria-hidden>
      <path d="M11.571 4.714h1.715v5.143H11.57zm4.715 0H18v5.143h-1.714zM6 0 1.714 4.286v15.428h5.143V24l4.286-4.286h3.428L22.286 12V0zm14.571 11.143-3.428 3.428h-3.429l-3 3v-3H6.857V1.714h13.714z" />
    </svg>
  );
}

/** Login / setup frame, like the shadcn login blocks: logo above a card. */
function Shell({ title, subtitle, children, wide, top }: { title: string; subtitle?: React.ReactNode; children: React.ReactNode; wide?: boolean; top?: React.ReactNode }) {
  return (
    <div className="flex min-h-svh flex-col">
      <main className="flex flex-1 items-center justify-center p-6 md:p-10">
        <div className={cn("flex w-full flex-col gap-6", wide ? "max-w-2xl" : "max-w-sm")}>
          <div className="flex justify-center">
            <Logo />
          </div>
          <Card>
            <CardHeader className="text-center">
              {top}
              <CardTitle className="text-xl">{title}</CardTitle>
              {subtitle && <CardDescription>{subtitle}</CardDescription>}
            </CardHeader>
            <CardContent>{children}</CardContent>
          </Card>
        </div>
      </main>
      <Footer />
    </div>
  );
}

/** Shown until the server said which state we're in (never a wrong screen for a moment). */
function Pending() {
  return (
    <div className="flex flex-col gap-5" aria-busy="true">
      <div className="grid gap-2">
        <Skeleton className="h-3.5 w-20" />
        <Skeleton className="h-8 w-full" />
      </div>
      <div className="grid gap-2">
        <Skeleton className="h-3.5 w-16" />
        <Skeleton className="h-8 w-full" />
      </div>
      <Skeleton className="h-8 w-full" />
    </div>
  );
}

function Field({ id, label, children, hint }: { id: string; label: string; children: React.ReactNode; hint?: string }) {
  return (
    <div className="grid gap-2">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  );
}

function ErrorBox({ msg }: { msg: string | null }) {
  if (!msg) return null;
  return (
    <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {msg}
    </p>
  );
}

function TwitchButton({ href }: { href: string }) {
  return (
    <>
      <div className="relative text-center text-sm after:absolute after:inset-0 after:top-1/2 after:z-0 after:flex after:items-center after:border-t after:border-border">
        <span className="relative z-10 bg-card px-2 text-muted-foreground">or</span>
      </div>
      <Button variant="outline" className="w-full" render={<a href={href} />}>
        <TwitchIcon /> Continue with Twitch
      </Button>
    </>
  );
}

export function nextPath(n: string | null = typeof window === "undefined" ? null : new URLSearchParams(window.location.search).get("next")): string {
  if (!n || n.length >= 512 || !n.startsWith("/") || n.startsWith("//") || n.startsWith("/api")) return "/";
  return /^[\x21-\x7e]+$/.test(n) && !n.includes("\\") ? n : "/";
}

function useRedirects(mode: "login" | "register" | "setup", active = true) {
  const { data } = useInstance();
  const router = useRouter();
  useEffect(() => {
    if (!data || !active) return;
    if (data.user) router.replace(nextPath());
    else if (data.setup && mode !== "setup") router.replace("/setup");
    else if (!data.setup && mode === "setup") router.replace("/login");
  }, [data, mode, router, active]);
  return data;
}

/** First start: step 1 proves you run the server (code from the log), step 2 creates the admin. */
export function SetupFlow() {
  const [rawStep, setStep] = useState(1);
  const stepParam = useParam("step");
  const fromLink = useParam("code");
  const { data: inst } = useInstance();
  const step = rawStep === 1 && stepParam === "3" && inst?.user ? 3 : rawStep;
  const data = useRedirects("setup", step === 1 && stepParam !== "3");
  const qc = useQueryClient();
  const router = useRouter();
  const [code, setCode] = useState("");
  const [verified, setVerified] = useState<string | null>(null);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const autoChecked = useRef(false);

  const check = async (c: string) => {
    setBusy(true);
    setMsg(null);
    try {
      await api("/setup/check", { body: { code: c.trim() } });
      setVerified(c.trim());
      setStep(2);
    } catch (e) {
      setMsg((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    if (fromLink && !autoChecked.current) {
      autoChecked.current = true;
      void check(fromLink);
    }
  }, [fromLink]);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setMsg(null);
    try {
      await api("/setup", { body: { code: verified ?? "", username, password } });
      setStep(3);
      void qc.invalidateQueries({ queryKey: ["instance"] });
    } catch (er) {
      setMsg((er as Error).message);
    } finally {
      setBusy(false);
    }
  };

  if (!inst || (fromLink && rawStep === 1 && !msg)) {
    return (
      <Shell title="Set up your uploader" subtitle="One moment…">
        <Pending />
      </Shell>
    );
  }
  const subtitles = ["Almost there.", "Create your admin account.", "Choose who can sign up and how people log in.", "You're ready to upload."];
  const steps = [1, 2, 3, 4];
  return (
    <Shell
      title={step === 4 ? "All set" : "Set up your uploader"}
      subtitle={subtitles[step - 1]}
      wide={step === 3}
      top={
        <div className="mx-auto mb-2 flex w-24 items-center gap-1.5" aria-label={`Step ${steps.indexOf(step) + 1} of ${steps.length}`}>
          {steps.map((s) => (
            <div key={s} className={cn("h-1 flex-1 rounded-full bg-muted", s <= step && "bg-foreground")} />
          ))}
        </div>
      }
    >
      {step === 3 ? (
        <InstanceForm compact submitLabel="Save and continue" onSaved={() => setStep(4)} />
      ) : step === 4 ? (
        <div className="flex flex-col gap-3">
          <Button className="w-full" onClick={() => router.replace("/")}>
            Start uploading <ArrowRight />
          </Button>
          <Button variant="outline" className="w-full" onClick={() => router.replace("/settings")}>
            Connect ShareX or Chatterino
          </Button>
        </div>
      ) : step === 1 ? (
        <div className="flex flex-col gap-4">
          <div className="flex items-start gap-3 rounded-lg border bg-muted/40 p-3 text-left text-sm">
            <Terminal className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            <span>
              <span className="font-medium">Enter the setup code from the server log.</span>
              <span className="mt-1 block text-muted-foreground">
                It proves you run this server. With Docker: <code className="font-mono text-xs">docker compose logs uploader</code>. The log also has a link that fills it in.
              </span>
            </span>
          </div>
          <form
            className="flex flex-col gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (code.trim()) void check(code);
            }}
          >
            <ErrorBox msg={msg} />
            <Input autoFocus placeholder="Setup code" value={code} onChange={(e) => setCode(e.target.value)} className="text-center font-mono tracking-wider" autoComplete="off" spellCheck={false} />
            <Button type="submit" className="w-full" disabled={!code.trim() || busy}>
              Continue <ArrowRight />
            </Button>
          </form>
        </div>
      ) : (
        <div className="flex flex-col gap-4">
          {verified && (
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <ShieldCheck className="size-4" /> Code accepted
            </p>
          )}
          <ErrorBox msg={msg} />
          <form className="flex flex-col gap-5" onSubmit={create}>
            <Field id="setup-user" label="Username">
              <Input id="setup-user" autoFocus autoComplete="username" value={username} onChange={(e) => setUsername(e.target.value)} required />
            </Field>
            <Field id="setup-pass" label="Password" hint="At least 8 characters">
              <Input id="setup-pass" type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} minLength={8} required />
            </Field>
            <Button type="submit" className="w-full" disabled={busy || password.length < 8 || !username}>
              Create admin account
            </Button>
          </form>
          {data?.twitch && <TwitchButton href={`/api/auth/twitch?setup=${encodeURIComponent(verified ?? "")}`} />}
        </div>
      )}
    </Shell>
  );
}

/** Log in and sign up. */
export function AuthCard({ mode }: { mode: "login" | "register" }) {
  const data = useRedirects(mode);
  const qc = useQueryClient();
  const router = useRouter();
  const error = useParam("error");
  const invite = useParam("invite");
  const nextParam = useParam("next");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const inv = useQuery({
    queryKey: ["invite", invite],
    queryFn: () => api<{ valid: boolean; username?: string | null; admin?: boolean }>(`/invites/${encodeURIComponent(invite!)}`),
    enabled: mode === "register" && !!invite,
    staleTime: Infinity,
  });
  const reserved = inv.data?.valid ? (inv.data.username ?? null) : null;
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setMsg(null);
    try {
      if (mode === "login") await api("/auth/login", { body: { username, password } });
      else await api("/auth/register", { body: { username: reserved ?? username, password, invite } });
      await qc.invalidateQueries({ queryKey: ["instance"] });
      router.replace(nextPath());
    } catch (err) {
      setMsg((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const invited = !!inv.data?.valid;
  const canRegister = !!data && (data.registration === "open" || invited);
  const twitchParams = new URLSearchParams();
  if (mode === "register" && invite) twitchParams.set("invite", invite);
  const next = nextPath(nextParam);
  if (next !== "/") twitchParams.set("next", next);
  const twitchUrl = `/api/auth/twitch${twitchParams.size ? `?${twitchParams}` : ""}`;
  const waiting = !data || data.user || (mode === "register" && !!invite && inv.isPending);
  const title = mode === "login" ? "Welcome back" : invited ? "You're invited" : "Create your account";
  const subtitle = mode === "login" ? "Log in to continue" : invited ? (inv.data?.admin ? "Set up your admin account" : "Pick a password or use Twitch") : "It only takes a moment";
  return (
    <Shell title={title} subtitle={subtitle}>
      {waiting ? (
        <Pending />
      ) : (
        <div className="flex flex-col gap-5">
          <ErrorBox msg={msg ?? error ?? (mode === "register" && invite && !invited ? "This invite link is not valid any more." : null)} />
          {mode === "register" && !canRegister ? (
            <p className="text-center text-sm text-muted-foreground">{data.registration === "invite" ? "You need an invite link to sign up." : "Sign up is closed."}</p>
          ) : (
            <form className="flex flex-col gap-5" onSubmit={submit}>
              <Field id="auth-user" label="Username">
                <Input
                  id="auth-user"
                  autoFocus={!reserved}
                  autoComplete="username"
                  value={reserved ?? username}
                  readOnly={!!reserved}
                  onChange={(e) => setUsername(e.target.value)}
                  className={reserved ? "bg-muted" : undefined}
                  required
                />
              </Field>
              <Field id="auth-pass" label="Password" hint={mode === "register" ? "At least 8 characters" : undefined}>
                <Input
                  id="auth-pass"
                  type="password"
                  autoFocus={!!reserved}
                  autoComplete={mode === "login" ? "current-password" : "new-password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  minLength={mode === "login" ? 1 : 8}
                />
              </Field>
              <Button type="submit" className="w-full" disabled={busy}>
                {mode === "login" ? "Log in" : "Create account"}
              </Button>
            </form>
          )}
          {data.twitch && (mode === "login" || canRegister) && <TwitchButton href={twitchUrl} />}
          {mode === "login" && data.registration === "open" && (
            <p className="text-center text-sm text-muted-foreground">
              No account yet?{" "}
              <Link href="/register" className="font-medium text-foreground hover:underline">
                Sign up
              </Link>
            </p>
          )}
          {mode === "register" && (
            <p className="text-center text-sm text-muted-foreground">
              Already have one?{" "}
              <Link href="/login" className="font-medium text-foreground hover:underline">
                Log in
              </Link>
            </p>
          )}
        </div>
      )}
    </Shell>
  );
}
