import Link from "next/link";

import { LogoMark } from "@/components/app/logo";

export default function NotFound() {
  return (
    <div className="flex min-h-svh flex-col items-center justify-center gap-3 p-4 text-center">
      <LogoMark className="size-8" />
      <p className="font-mono text-5xl font-semibold tracking-tight">404</p>
      <p className="text-muted-foreground">This page doesn&apos;t exist (any more).</p>
      <Link href="/" className="mt-2 text-sm font-medium hover:underline">
        Back home
      </Link>
    </div>
  );
}
