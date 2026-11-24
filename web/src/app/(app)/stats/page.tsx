"use client";

import { useQuery } from "@tanstack/react-query";
import { Eye, HardDrive, Upload } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import { PageHeader, useApp } from "@/components/app/app-shell";
import { ItemIcon } from "@/components/app/item-icon";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, type Item, type User, type ViewType } from "@/lib/api";
import { bytes } from "@/lib/format";
import { statsQuery } from "@/lib/queries";

type Stats = {
  count: number;
  size: number;
  views: number;
  users: number;
  quota: number | null;
  types: { type: ViewType; count: number; size: number }[];
  days: { day: number; count: number; size: number }[];
  top: Item[];
  disk: { total: number; free: number } | null;
};

const TYPE_LABEL: Record<string, string> = { image: "Images", video: "Videos", audio: "Audio", pdf: "PDFs", text: "Text", link: "Links", other: "Other" };

function Stat({ label, value, sub, icon: Icon }: { label: string; value: string; sub?: React.ReactNode; icon: typeof Upload }) {
  return (
    <Card size="sm">
      <CardContent className="flex flex-col gap-1">
        <span className="flex items-center justify-between text-xs text-muted-foreground">
          {label}
          <span className="flex size-7 items-center justify-center rounded-md border text-muted-foreground">
            <Icon className="size-3.5" />
          </span>
        </span>
        <span className="text-2xl font-semibold tabular-nums">{value}</span>
        {sub}
      </CardContent>
    </Card>
  );
}

function Bar({ value, max }: { value: number; max: number }) {
  return (
    <div className="h-1.5 overflow-hidden rounded-full bg-muted">
      <div className="h-full rounded-full bg-foreground" style={{ width: `${max ? Math.min(100, (value / max) * 100) : 0}%` }} />
    </div>
  );
}

function StatsView({ user }: { user: User }) {
  const [scope, setScope] = useState("me");
  const [days, setDays] = useState("30");
  const stats = useQuery({ ...statsQuery(scope, days), queryFn: () => api<Stats>(`/stats?days=${days}${scope === "all" ? "&scope=all" : ""}`) });
  const s = stats.data;
  const chart = s?.days.map((d) => ({ date: new Date(d.day * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }), uploads: d.count, size: d.size })) ?? [];
  const maxType = Math.max(1, ...(s?.types.map((t) => t.count) ?? [1]));
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Stats"
        description="Uploads, storage and views."
        actions={
        <div className="flex gap-2">
          <Tabs value={days} onValueChange={(v) => setDays(String(v))}>
            <TabsList>
              <TabsTrigger value="7">7d</TabsTrigger>
              <TabsTrigger value="30">30d</TabsTrigger>
              <TabsTrigger value="90">90d</TabsTrigger>
              <TabsTrigger value="365">1y</TabsTrigger>
            </TabsList>
          </Tabs>
          {user.role === "admin" && (
            <Tabs value={scope} onValueChange={(v) => setScope(String(v))}>
              <TabsList>
                <TabsTrigger value="me">Mine</TabsTrigger>
                <TabsTrigger value="all">Everyone</TabsTrigger>
              </TabsList>
            </Tabs>
          )}
        </div>
        }
      />
      {!s ? (
        <div className="grid gap-3 sm:grid-cols-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            <Stat icon={Upload} label="Uploads" value={s.count.toLocaleString()} sub={scope === "all" && <span className="text-xs text-muted-foreground">{s.users} users</span>} />
            <Stat
              icon={HardDrive}
              label="Storage"
              value={bytes(s.size)}
              sub={
                s.quota ? (
                  <div className="flex flex-col gap-1">
                    <Bar value={s.size} max={s.quota} />
                    <span className="text-xs text-muted-foreground">of {bytes(s.quota)}</span>
                  </div>
                ) : s.disk ? (
                  <div className="flex flex-col gap-1">
                    <Bar value={s.disk.total - s.disk.free} max={s.disk.total} />
                    <span className="text-xs text-muted-foreground">{bytes(s.disk.free)} free on disk</span>
                  </div>
                ) : null
              }
            />
            <Stat icon={Eye} label="Views" value={s.views.toLocaleString()} />
          </div>
          <Card>
            <CardHeader>
              <CardTitle>Uploads per day</CardTitle>
            </CardHeader>
            <CardContent className="h-56 px-2">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chart} margin={{ left: 0, right: 8, top: 4 }}>
                  <defs>
                    <linearGradient id="fill" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="var(--foreground)" stopOpacity={0.12} />
                      <stop offset="100%" stopColor="var(--foreground)" stopOpacity={0} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid vertical={false} stroke="var(--border)" />
                  <XAxis dataKey="date" tickLine={false} axisLine={false} fontSize={11} stroke="var(--muted-foreground)" minTickGap={24} />
                  <YAxis allowDecimals={false} tickLine={false} axisLine={false} fontSize={11} width={28} stroke="var(--muted-foreground)" />
                  <Tooltip
                    contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }}
                    formatter={(v, n, p) => (n === "uploads" ? [`${v} (${bytes(p.payload.size)})`, "Uploads"] : [v, n])}
                  />
                  <Area type="monotone" dataKey="uploads" stroke="var(--foreground)" strokeWidth={1.5} fill="url(#fill)" isAnimationActive={false} />
                </AreaChart>
              </ResponsiveContainer>
            </CardContent>
          </Card>
          <div className="grid gap-3 md:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>By type</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-3">
                {s.types.length === 0 && <p className="text-sm text-muted-foreground">No uploads yet</p>}
                {s.types.map((t) => (
                  <div key={t.type} className="flex flex-col gap-1">
                    <div className="flex items-center justify-between text-sm">
                      <span className="flex items-center gap-2">
                        <ItemIcon type={t.type} className="size-4 text-muted-foreground" />
                        {TYPE_LABEL[t.type] ?? t.type}
                      </span>
                      <span className="text-muted-foreground tabular-nums">
                        {t.count} · {bytes(t.size)}
                      </span>
                    </div>
                    <Bar value={t.count} max={maxType} />
                  </div>
                ))}
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle>Most viewed</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-1">
                {s.top.length === 0 && <p className="text-sm text-muted-foreground">No views yet</p>}
                {s.top.map((i) => (
                  <Link key={i.id} href={i.url} target="_blank" className="flex items-center gap-3 rounded-md p-1.5 hover:bg-muted">
                    <span className="flex size-9 shrink-0 items-center justify-center overflow-hidden rounded bg-muted">
                      {i.thumb_url ? (
                        // eslint-disable-next-line @next/next/no-img-element
                        <img src={i.thumb_url} alt="" className="size-full object-cover" loading="lazy" />
                      ) : (
                        <ItemIcon type={i.type} className="size-4 text-muted-foreground" />
                      )}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-sm">{i.name}</span>
                    <span className="text-sm text-muted-foreground tabular-nums">{i.views.toLocaleString()}</span>
                  </Link>
                ))}
              </CardContent>
            </Card>
          </div>
        </>
      )}
    </div>
  );
}

export default function StatsPage() {
  const { user } = useApp();
  return <StatsView user={user} />;
}
