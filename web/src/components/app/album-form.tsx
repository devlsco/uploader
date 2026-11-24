"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { api, type Album } from "@/lib/api";

export function AlbumForm({ album, open, onClose, onSaved }: { album?: Pick<Album, "id" | "title" | "description" | "public">; open: boolean; onClose: () => void; onSaved?: () => void }) {
  const qc = useQueryClient();
  const [title, setTitle] = useState(album?.title ?? "");
  const [description, setDescription] = useState(album?.description ?? "");
  const [pub, setPub] = useState(album?.public ?? true);
  const save = useMutation({
    mutationFn: () => (album ? api(`/albums/${album.id}`, { method: "PATCH", body: { title, description, public: pub } }) : api("/albums", { body: { title, description, public: pub } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["albums"] });
      toast.success(album ? "Saved" : "Album created");
      onSaved?.();
      onClose();
    },
    onError: (e) => toast.error(e.message),
  });
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{album ? "Edit album" : "New album"}</DialogTitle>
        </DialogHeader>
        <form
          id="album-form"
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate();
          }}
        >
          <Input autoFocus placeholder="Title" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={80} required />
          <Textarea placeholder="Description (optional)" value={description} onChange={(e) => setDescription(e.target.value)} maxLength={500} />
          <Label className="flex items-center justify-between font-normal">
            Anyone with the link can see it
            <Switch checked={pub} onCheckedChange={setPub} />
          </Label>
        </form>
        <DialogFooter>
          <Button type="submit" form="album-form" disabled={!title.trim() || save.isPending}>
            {album ? "Save" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

