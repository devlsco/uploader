"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, MoreHorizontal, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { PageHeader, useApp } from "@/components/app/app-shell";
import { InstanceForm } from "@/components/app/instance-form";
import { OptionSelect } from "@/components/app/option-select";
import { UpdateButton, useVersion } from "@/components/app/update";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type User } from "@/lib/api";
import { copy } from "@/lib/copy";
import { ago, bytes } from "@/lib/format";

type AdminUser = User & { items: number };
type Invite = { code: string; uses_left: number; expires_at: number | null; created_at: number; url: string; username: string | null; admin: boolean };

function Users({ me }: { me: User }) {
  const qc = useQueryClient();
  const users = useQuery({ queryKey: ["admin", "users"], queryFn: () => api<AdminUser[]>("/admin/users") });
  const [inviting, setInviting] = useState(false);
  const [quotaFor, setQuotaFor] = useState<AdminUser | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["admin", "users"] });
  const patch = useMutation({
    mutationFn: ({ id, body }: { id: number; body: Record<string, unknown> }) => api(`/admin/users/${id}`, { method: "PATCH", body }),
    onSuccess: refresh,
    onError: (e) => toast.error(e.message),
  });
  const del = useMutation({
    mutationFn: (id: number) => api(`/admin/users/${id}`, { method: "DELETE" }),
    onSuccess: () => {
      toast.success("User and files deleted");
      void refresh();
    },
    onError: (e) => toast.error(e.message),
  });
  const reset = useMutation({
    mutationFn: (id: number) => api<{ password: string }>(`/admin/users/${id}/password`, { method: "POST" }),
    onSuccess: (r) => copy(r.password, "New password"),
    onError: (e) => toast.error(e.message),
  });
  return (
    <div className="flex flex-col gap-6">
      <div className="flex justify-end">
        <Button onClick={() => setInviting(true)}>
          <Plus /> Invite
        </Button>
      </div>
      <div className="flex flex-col divide-y rounded-lg border">
        {users.data?.map((u) => (
          <div key={u.id} className="flex items-center gap-3 p-3">
            <Avatar className="size-8">
              {u.avatar_url && <AvatarImage src={u.avatar_url} alt="" />}
              <AvatarFallback>{u.username.slice(0, 1).toUpperCase()}</AvatarFallback>
            </Avatar>
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-2 truncate text-sm font-medium">
                {u.username}
                {u.owner ? <Badge variant="secondary">Owner</Badge> : u.role === "admin" && <Badge variant="secondary">Admin</Badge>}
                {u.disabled && <Badge variant="destructive">Disabled</Badge>}
              </p>
              <p className="text-xs text-muted-foreground">
                {u.items} files · {bytes(u.used_bytes)}
                {u.quota_mb ? ` of ${bytes(u.quota_mb * 1048576)}` : ""} · joined {ago(u.created_at)}
              </p>
            </div>
            {/* the owner can only be changed by themselves (the server checks it too) */}
            {u.id !== me.id && !u.owner && (
              <DropdownMenu>
                <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Actions" />}>
                  <MoreHorizontal />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => patch.mutate({ id: u.id, body: { role: u.role === "admin" ? "user" : "admin" } })}>{u.role === "admin" ? "Make user" : "Make admin"}</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setQuotaFor(u)}>Storage limit</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => reset.mutate(u.id)}>New password</DropdownMenuItem>
                  <DropdownMenuItem onClick={() => patch.mutate({ id: u.id, body: { disabled: !u.disabled } })}>{u.disabled ? "Enable" : "Disable"}</DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem variant="destructive" onClick={() => confirm(`Delete ${u.username} and all files?`) && del.mutate(u.id)}>
                    <Trash2 /> Delete
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>
        ))}
      </div>
      <PendingInvites />
      {inviting && <InviteDialog onClose={() => setInviting(false)} />}
      {quotaFor && <Quota user={quotaFor} onClose={() => setQuotaFor(null)} onSave={(q) => patch.mutate({ id: quotaFor.id, body: { quota_mb: q } })} />}
    </div>
  );
}

function Quota({ user, onClose, onSave }: { user: AdminUser; onClose: () => void; onSave: (q: number | null) => void }) {
  const [v, setV] = useState(user.quota_mb ? String(user.quota_mb) : "");
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Storage limit for {user.username}</DialogTitle>
        </DialogHeader>
        <div className="flex items-center gap-2">
          <Input autoFocus type="number" min={0} placeholder="Default" value={v} onChange={(e) => setV(e.target.value)} />
          <span className="text-sm text-muted-foreground">MB</span>
        </div>
        <DialogFooter>
          <Button
            onClick={() => {
              onSave(v === "" ? null : Number(v));
              onClose();
            }}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Invite someone: optional name and admin rights, they choose a password or use Twitch. */
function InviteDialog({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient();
  const [username, setUsername] = useState("");
  const [admin, setAdmin] = useState(false);
  const [days, setDays] = useState("7");
  const [uses, setUses] = useState("1");
  const [link, setLink] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () => api<{ url: string }>("/admin/invites", { body: { username: username.trim() || undefined, admin, days: Number(days) || null, uses: Number(uses) || 1 } }),
    onSuccess: (r) => {
      setLink(r.url);
      void copy(r.url, "Invite link");
      void qc.invalidateQueries({ queryKey: ["admin", "invites"] });
    },
    onError: (e) => toast.error(e.message),
  });
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{link ? "Invite ready" : "Invite someone"}</DialogTitle>
        </DialogHeader>
        {link ? (
          <div className="flex flex-col gap-3">
            <p className="text-sm text-muted-foreground">Send this link. It was copied already.</p>
            <div className="flex gap-2">
              <Input readOnly value={link} className="font-mono text-xs" onFocus={(e) => e.target.select()} />
              <Button variant="outline" size="icon" aria-label="Copy" onClick={() => copy(link, "Invite link")}>
                <Copy />
              </Button>
            </div>
            <DialogFooter>
              <Button onClick={onClose}>Done</Button>
            </DialogFooter>
          </div>
        ) : (
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              create.mutate();
            }}
          >
            <div className="grid gap-2">
              <Label htmlFor="inv-name">Username</Label>
              <Input id="inv-name" autoFocus placeholder="Optional, they choose one otherwise" value={username} onChange={(e) => setUsername(e.target.value)} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="grid gap-2">
                <Label>Valid for</Label>
                <OptionSelect value={days} options={[{ value: "1", label: "1 day" }, { value: "7", label: "7 days" }, { value: "30", label: "30 days" }, { value: "0", label: "No expiry" }]} onChange={setDays} />
              </div>
              <div className="grid gap-2">
                <Label>People</Label>
                {username.trim() ? (
                  <Input disabled value="1" />
                ) : (
                  <OptionSelect value={uses} options={["1", "5", "10", "100"].map((u) => ({ value: u, label: u === "1" ? "One person" : `${u} people` }))} onChange={setUses} />
                )}
              </div>
            </div>
            <Label className="flex items-center justify-between font-normal">
              Admin
              <Switch checked={admin} onCheckedChange={setAdmin} />
            </Label>
            <DialogFooter>
              <Button type="submit" disabled={create.isPending}>
                Create invite link
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

function PendingInvites() {
  const qc = useQueryClient();
  const invites = useQuery({ queryKey: ["admin", "invites"], queryFn: () => api<Invite[]>("/admin/invites") });
  const del = useMutation({
    mutationFn: (code: string) => api(`/admin/invites/${code}`, { method: "DELETE" }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["admin", "invites"] }),
  });
  if (!invites.data?.length) return null;
  return (
    <div className="flex flex-col gap-2">
      <h2 className="text-sm font-medium">Open invites</h2>
      <div className="flex flex-col divide-y rounded-lg border">
        {invites.data.map((i) => (
          <div key={i.code} className="flex items-center gap-3 p-3">
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-2 truncate text-sm">
                {i.username ?? "Anyone with the link"}
                {i.admin && <Badge variant="secondary">Admin</Badge>}
              </p>
              <p className="text-xs text-muted-foreground">
                {i.uses_left > 1 ? `${i.uses_left} sign ups left` : "One sign up"}
                {i.expires_at ? ` · ends ${ago(i.expires_at)}` : ""}
              </p>
            </div>
            <Button variant="ghost" size="icon-sm" aria-label="Copy link" onClick={() => copy(i.url, "Invite link")}>
              <Copy />
            </Button>
            <Button variant="ghost" size="icon-sm" aria-label="Delete invite" onClick={() => del.mutate(i.code)}>
              <Trash2 />
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}

function About() {
  const { data: v } = useVersion();
  if (!v) return null;
  return (
    <div className="flex flex-col gap-4 text-sm">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex-1">
          <p className="font-medium">Version {v.current}</p>
          <p className="text-muted-foreground">{v.latest ? (v.update ? `Version ${v.latest} is available` : "Up to date") : "Could not check for updates"}</p>
        </div>
        {v.update && <UpdateButton v={v} />}
      </div>
      <a className="flex items-center gap-1.5 text-muted-foreground hover:text-foreground" href={`https://github.com/${v.repo}`} target="_blank" rel="noreferrer">
        github.com/{v.repo}
      </a>
    </div>
  );
}

export default function AdminPage() {
  const { user } = useApp();
  if (user.role !== "admin") return <p className="text-sm text-muted-foreground">Admins only.</p>;
  return (
    <>
      <PageHeader title="Admin" description="People and settings." />
      <Tabs defaultValue="users" className="gap-6">
        <TabsList>
          <TabsTrigger value="users">Users</TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>
        <TabsContent value="users">
          <Users me={user} />
        </TabsContent>
        <TabsContent value="settings" className="flex flex-col gap-6">
          <Card>
            <CardContent>
              <About />
            </CardContent>
          </Card>
          <Card>
            <CardContent>
              <InstanceForm />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </>
  );
}
