"use client";

import { Download, QrCode } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { downloadUrl, qrPng } from "@/lib/qr";

import { QrImage } from "./qr-image";

/** A "QR" button that shows the QR code of a link. */
export function QrButton({ url, name, size = "sm" }: { url: string; name: string; size?: "sm" | "icon-sm" }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="outline" size={size} onClick={() => setOpen(true)} aria-label="QR code">
        <QrCode /> {size === "sm" && "QR"}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-xs">
          <DialogTitle className="truncate">{name}</DialogTitle>
          <QrImage text={url} className="border bg-white p-2" />
          <p className="truncate text-center text-xs text-muted-foreground">{url}</p>
          <Button onClick={async () => downloadUrl(await qrPng(url), `qr-${name.replace(/[^\w.-]+/g, "_")}.png`)}>
            <Download /> Download PNG
          </Button>
        </DialogContent>
      </Dialog>
    </>
  );
}
