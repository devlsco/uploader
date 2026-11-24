"use client";

import { useEffect, useState } from "react";

import { Skeleton } from "@/components/ui/skeleton";
import { QR_DEFAULTS, type QrOptions, qrSvg } from "@/lib/qr";
import { cn } from "cn";

/** Renders a QR code as inline SVG (sharp at any size). */
export function QrImage({ text, options = QR_DEFAULTS, className }: { text: string; options?: QrOptions; className?: string }) {
  const [svg, setSvg] = useState<{ text: string; svg: string } | null>(null);
  const key = `${text}|${options.margin}|${options.dark}|${options.light}|${options.ecc}`;
  useEffect(() => {
    if (!text) return;
    let cancelled = false;
    qrSvg(text, options)
      .then((s) => !cancelled && setSvg({ text: key, svg: s }))
      .catch(() => !cancelled && setSvg({ text: key, svg: "" }));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  if (!text || !svg) return <Skeleton className={cn("aspect-square w-full", className)} />;
  if (!svg.svg) return <div className={cn("flex aspect-square w-full items-center justify-center rounded-lg border text-sm text-muted-foreground", className)}>Too much text for a QR code</div>;
  return <div className={cn("aspect-square w-full overflow-hidden rounded-lg [&_svg]:size-full", className)} dangerouslySetInnerHTML={{ __html: svg.svg }} />;
}
