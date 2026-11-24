import { cn } from "cn";

export function LogoMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={cn("size-6 shrink-0", className)} aria-hidden>
      <rect x="2.75" y="6.75" width="14.5" height="14.5" rx="3.25" fill="none" stroke="currentColor" strokeWidth="1.5" opacity=".45" />
      <rect x="7" y="2" width="15" height="15" rx="3.5" fill="currentColor" />
      <path d="M14.5 13V6.5m-3 3 3-3 3 3" fill="none" stroke="var(--background)" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** Mark and name; static, so it is there in the first frame on every instance. */
export function Logo({ className }: { className?: string }) {
  return (
    <span className={cn("flex items-center gap-2", className)}>
      <LogoMark />
      <span className="text-[15px] font-semibold tracking-tight">Uploader</span>
    </span>
  );
}

export const REPO_URL = "https://github.com/devlsco/uploader";

export function GithubIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" className={cn("size-4", className)} fill="currentColor" aria-hidden>
      <path d="M12 .3a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2c-3.3.7-4-1.6-4-1.6-.6-1.4-1.4-1.8-1.4-1.8-1-.7.1-.7.1-.7 1.2 0 1.9 1.2 1.9 1.2 1 1.8 2.8 1.3 3.5 1 0-.8.4-1.3.7-1.6-2.7-.3-5.5-1.3-5.5-6 0-1.2.5-2.3 1.3-3.1-.2-.4-.6-1.6 0-3.2 0 0 1-.3 3.4 1.2a11.5 11.5 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.6 1.6.2 2.8 0 3.2.9.8 1.3 1.9 1.3 3.2 0 4.6-2.8 5.6-5.5 5.9.5.4.9 1 .9 2.2v3.3c0 .3.1.7.8.6A12 12 0 0 0 12 .3" />
    </svg>
  );
}

export function Footer({ version }: { version?: string }) {
  return (
    <footer className="mx-auto flex w-full max-w-5xl items-center justify-center gap-3 px-4 py-6 text-xs text-muted-foreground">
      <span>Uploader{version ? ` v${version}` : ""}</span>
      <span aria-hidden>·</span>
      <a href={REPO_URL} target="_blank" rel="noreferrer" className="flex items-center gap-1.5 transition-colors hover:text-foreground">
        <GithubIcon className="size-3.5" /> devlsco/uploader
      </a>
    </footer>
  );
}
