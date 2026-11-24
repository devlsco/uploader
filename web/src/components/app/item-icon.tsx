import { File, FileAudio, FileCode, FileImage, FileText, FileVideo, Link2 } from "lucide-react";

import type { ViewType } from "@/lib/api";

export function ItemIcon({ type, className }: { type: ViewType; className?: string }) {
  const I = { image: FileImage, video: FileVideo, audio: FileAudio, pdf: FileText, text: FileCode, link: Link2, other: File }[type] ?? File;
  return <I className={className} />;
}
