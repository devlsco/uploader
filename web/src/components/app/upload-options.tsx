"use client";

import { useQuery } from "@tanstack/react-query";
import { Check, ChevronDown, Clock, Flame, FolderClosed, Lock, X } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { UploadOptions } from "@/lib/api";
import { EXPIRY } from "@/lib/format";
import { albumsQuery } from "@/lib/queries";
import { cn } from "cn";

export function useAlbums() {
  return useQuery(albumsQuery);
}

const chip = "inline-flex h-7 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground data-[on=true]:border-foreground/30 data-[on=true]:bg-muted data-[on=true]:text-foreground";

function ChipSelect({ icon: Icon, value, options, onChange, label }: { icon: typeof Clock; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void; label: string }) {
  const current = options.find((o) => o.value === value) ?? options[0];
  const on = value !== options[0].value;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<button type="button" className={chip} data-on={on} aria-label={label} />}>
        <Icon className="size-3.5" />
        {current.label}
        <ChevronDown className="size-3.5 opacity-60" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-48">
        {options.map((o) => (
          <DropdownMenuItem key={o.value} onClick={() => onChange(o.value)}>
            <Check className={cn("size-4", o.value !== value && "opacity-0")} />
            {o.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function PasswordChip({ value, onChange }: { value?: string; onChange: (v?: string) => void }) {
  const [draft, setDraft] = useState("");
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger render={<button type="button" className={chip} data-on={!!value} />}>
        <Lock className="size-3.5" />
        {value ? "Password set" : "Password"}
        {value && (
          <span
            role="button"
            tabIndex={0}
            aria-label="Remove password"
            onClick={(e) => {
              e.stopPropagation();
              onChange(undefined);
            }}
            className="-mr-1 rounded p-0.5 hover:bg-background"
          >
            <X className="size-3" />
          </span>
        )}
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64">
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            onChange(draft || undefined);
            setDraft("");
            setOpen(false);
          }}
        >
          <Input autoFocus type="password" autoComplete="new-password" placeholder="Password" value={draft} onChange={(e) => setDraft(e.target.value)} />
          <Button type="submit" size="sm" disabled={!draft}>
            Set
          </Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}

/** Expiry, album, password and "view once" as small chips. */
export function UploadOptionsBar({ value, onChange, album = true, className }: { value: UploadOptions; onChange: (v: UploadOptions) => void; album?: boolean; className?: string }) {
  const albums = useAlbums();
  const set = (p: Partial<UploadOptions>) => onChange({ ...value, ...p });
  return (
    <div className={cn("flex flex-wrap items-center gap-2", className)}>
      <ChipSelect
        icon={Clock}
        label="Expiry"
        value={value.expires ?? "never"}
        options={EXPIRY.map((e) => ({ value: e.value, label: e.value === "never" ? "Keep forever" : `Delete after ${e.label}` }))}
        onChange={(v) => set({ expires: v })}
      />
      {album && (
        <ChipSelect
          icon={FolderClosed}
          label="Album"
          value={value.album ?? ""}
          options={[{ value: "", label: "No album" }, ...(albums.data ?? []).map((a) => ({ value: a.id, label: a.title }))]}
          onChange={(v) => set({ album: v || undefined })}
        />
      )}
      <PasswordChip value={value.password} onChange={(password) => set({ password })} />
      <button type="button" className={chip} data-on={!!value.burn} onClick={() => set({ burn: !value.burn })} aria-pressed={!!value.burn}>
        <Flame className="size-3.5" />
        View once
      </button>
    </div>
  );
}

const KEY = "upload-options";

export function loadOptions(): UploadOptions {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return { expires: v.expires, album: v.album };
  } catch {
    return {};
  }
}

export function saveOptions(o: UploadOptions) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ expires: o.expires, album: o.album }));
  } catch {
    /* private mode */
  }
}
