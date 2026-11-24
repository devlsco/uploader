"use client";

import { useQueryClient } from "@tanstack/react-query";
import { BarChart3, FolderOpen, Images, LogOut, Moon, QrCode, Settings, Shield, Sun, Upload } from "lucide-react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useTheme } from "next-themes";
import { createContext, useContext, useEffect, useMemo } from "react";
import { toast } from "sonner";

import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { useInstance } from "@/hooks/use-instance";
import { api, type Instance, type User } from "@/lib/api";
import { preload } from "@/lib/queries";
import { queue } from "@/lib/upload";

import { loadOptions } from "./upload-options";
import { cn } from "cn";

import { Footer, GithubIcon, Logo, LogoMark, REPO_URL } from "./logo";
import { UpdateBanner } from "./update";

const NAV = [
  { href: "/", label: "Upload", icon: Upload, mobile: true },
  { href: "/files", label: "Files", icon: FolderOpen, mobile: true },
  { href: "/albums", label: "Albums", icon: Images, mobile: true },
  { href: "/qr", label: "QR", icon: QrCode, mobile: true },
  { href: "/stats", label: "Stats", icon: BarChart3, mobile: false },
  { href: "/settings", label: "Settings", icon: Settings, mobile: true },
];

const isActive = (path: string | null, href: string) => (href === "/" ? path === "/" : !!path?.startsWith(href));

export function UserMenu({ user }: { user: User }) {
  const { resolvedTheme, setTheme } = useTheme();
  const qc = useQueryClient();
  const router = useRouter();
  const logout = async () => {
    await api("/auth/logout", { method: "POST" });
    qc.clear();
    router.replace("/login");
  };
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" size="icon" className="rounded-full" aria-label="Account" />}>
        <Avatar className="size-8 border">
          {user.avatar_url && <AvatarImage src={user.avatar_url} alt="" />}
          <AvatarFallback className="bg-muted text-xs font-medium">{user.username.slice(0, 1).toUpperCase()}</AvatarFallback>
        </Avatar>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <div className="px-2 py-1.5">
          <p className="truncate text-sm font-medium">{user.username}</p>
          <p className="text-xs text-muted-foreground capitalize">{user.role}</p>
        </div>
        <DropdownMenuSeparator />
        <DropdownMenuItem className="sm:hidden" onClick={() => router.push("/stats")}>
          <BarChart3 /> Stats
        </DropdownMenuItem>
        {user.role === "admin" && (
          <DropdownMenuItem className="sm:hidden" onClick={() => router.push("/admin")}>
            <Shield /> Admin
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onClick={() => setTheme(resolvedTheme === "dark" ? "light" : "dark")}>
          {resolvedTheme === "dark" ? <Sun /> : <Moon />}
          {resolvedTheme === "dark" ? "Light mode" : "Dark mode"}
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => window.open(REPO_URL, "_blank")}>
          <GithubIcon /> GitHub
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={logout}>
          <LogOut /> Log out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

type Ctx = { user: User; instance: Instance };
const AppContext = createContext<Ctx | null>(null);

/** The logged in user and the instance (only inside the app layout). */
export function useApp(): Ctx {
  const c = useContext(AppContext);
  if (!c) throw new Error("useApp outside the app layout");
  return c;
}

/** Title, short description and actions at the top of every app page. */
export function PageHeader({ title, description, actions }: { title: string; description?: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <div className="mb-6 flex flex-wrap items-end justify-between gap-3">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-1 text-sm text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

/** Rendered once for all app pages: the header stays in place while pages change. */
export function AppLayout({ children }: { children: React.ReactNode }) {
  const { data, isPending } = useInstance();
  const router = useRouter();
  const path = usePathname();
  const qc = useQueryClient();
  const loggedIn = !!data?.user;
  useEffect(() => {
    if (!data) return;
    if (data.setup) router.replace(`/setup${window.location.search}`);
    else if (!data.user) {
      const here = window.location.pathname + window.location.search;
      router.replace(here === "/" ? "/login" : `/login?next=${encodeURIComponent(here)}`);
    }
  }, [data, router]);
  useEffect(() => {
    if (loggedIn) preload(qc);
  }, [loggedIn, qc]);
  const ctx = useMemo(() => (data?.user ? { user: data.user, instance: data } : null), [data]);
  useEffect(() => {
    if (!loggedIn || path === "/") return;
    const paste = (e: ClipboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
      const files = Array.from(e.clipboardData?.files ?? []);
      if (!files.length) return;
      e.preventDefault();
      queue.add(files, loadOptions());
      toast.loading(files.length === 1 ? "Uploading…" : `Uploading ${files.length} files…`, { id: "paste-upload", duration: 2000 });
    };
    const over = (e: DragEvent) => {
      if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
    };
    const drop = (e: DragEvent) => {
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (!files.length) return;
      e.preventDefault();
      queue.add(files, loadOptions());
      toast.loading(files.length === 1 ? "Uploading…" : `Uploading ${files.length} files…`, { id: "paste-upload", duration: 2000 });
    };
    window.addEventListener("paste", paste);
    window.addEventListener("dragover", over);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("paste", paste);
      window.removeEventListener("dragover", over);
      window.removeEventListener("drop", drop);
    };
  }, [loggedIn, path]);
  const nav = data?.user?.role === "admin" ? [...NAV, { href: "/admin", label: "Admin", icon: Shield, mobile: false }] : NAV;
  return (
    <div className="flex min-h-svh flex-col">
      <header className="sticky top-0 z-40 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80">
        <div className="mx-auto flex h-14 max-w-5xl items-stretch gap-6 px-4">
          <Link href="/" className="flex items-center rounded-md outline-none focus-visible:ring-3 focus-visible:ring-ring/50">
            <Logo />
          </Link>
          {/* the tabs appear once the session is known (no Admin tab popping in later) */}
          <nav className="hidden items-stretch gap-1 sm:flex" aria-label="Main">
            {data?.user &&
              nav.map((n) => {
                const on = isActive(path, n.href);
                return (
                  <Link
                    key={n.href}
                    href={n.href}
                    aria-current={on ? "page" : undefined}
                    className={cn(
                      "relative flex items-center px-2.5 text-sm text-muted-foreground hover:text-foreground",
                      "after:absolute after:inset-x-2.5 after:-bottom-px after:h-0.5 after:rounded-full after:bg-foreground after:opacity-0",
                      on && "text-foreground after:opacity-100",
                    )}
                  >
                    {/* the tour frames just the label */}
                    <span data-tour={`nav-${n.href.slice(1) || "upload"}`}>{n.label}</span>
                  </Link>
                );
              })}
          </nav>
          <div className="ml-auto flex items-center">{data?.user ? <UserMenu user={data.user} /> : <div className="size-8" />}</div>
        </div>
      </header>
      {data?.user?.role === "admin" && <UpdateBanner />}
      <main className="mx-auto w-full max-w-5xl flex-1 px-4 pt-8 pb-28 sm:pb-12">
        {isPending || !ctx ? (
          <div className="flex justify-center pt-24">
            <LogoMark className="size-8 animate-pulse text-muted-foreground" />
          </div>
        ) : (
          <AppContext.Provider value={ctx}>{children}</AppContext.Provider>
        )}
      </main>
      <div className="hidden sm:block">
        <Footer version={data?.version} />
      </div>
      <nav className="fixed inset-x-0 bottom-0 z-40 flex h-[calc(3rem+env(safe-area-inset-bottom))] border-t bg-background/95 pb-[env(safe-area-inset-bottom)] backdrop-blur sm:hidden" aria-label="Main">
        {nav
          .filter((n) => n.mobile && data?.user)
          .map((n) => (
            <Link key={n.href} href={n.href} className={cn("relative flex flex-1 items-center justify-center py-3.5 text-[13px] font-medium text-muted-foreground", isActive(path, n.href) && "text-foreground")}>
              {isActive(path, n.href) && <span className="absolute inset-x-4 top-0 h-0.5 rounded-full bg-foreground" />}
              <span data-tour={`nav-${n.href.slice(1) || "upload"}`}>{n.label}</span>
            </Link>
          ))}
      </nav>
    </div>
  );
}

/** Header for public pages (item, album, delete). */
export function PublicHeader({ right }: { right?: React.ReactNode }) {
  const { data } = useInstance();
  return (
    <header className="sticky top-0 z-40 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80">
      <div className="mx-auto flex h-14 max-w-5xl items-center gap-2 px-4">
        <Link href="/">
          <Logo />
        </Link>
        <div className="ml-auto flex items-center gap-2">
          {right}
          {data?.user ? (
            <UserMenu user={data.user} />
          ) : (
            data && (
              <Button variant="ghost" size="sm" render={<Link href="/login" />}>
                Log in
              </Button>
            )
          )}
        </div>
      </div>
    </header>
  );
}
