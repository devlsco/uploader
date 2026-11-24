"use client";

import { Trash2 } from "lucide-react";
import { useState } from "react";

import { PublicHeader } from "@/components/app/app-shell";
import { Footer } from "@/components/app/logo";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { useParam, usePath } from "@/hooks/use-param";
import { api } from "@/lib/api";

export default function DeletePage() {
  const path = usePath();
  const id = path?.split("/")[2] ?? "";
  const token = useParam("t") ?? "";
  const [state, setState] = useState<"ask" | "busy" | "done">("ask");
  const [err, setErr] = useState<string | null>(null);
  return (
    <div className="flex min-h-svh flex-col">
      <PublicHeader />
      <main className="flex flex-1 items-center justify-center p-4">
        <Card className="w-full max-w-sm">
          <CardContent className="flex flex-col items-center gap-4 py-4 text-center">
            <span className="flex size-12 items-center justify-center rounded-md border border-destructive/30 bg-destructive/10 text-destructive">
              <Trash2 className="size-5" />
            </span>
            {state === "done" ? (
              <p className="font-medium">Deleted</p>
            ) : (
              <>
                <p className="font-medium">Delete this upload?</p>
                <p className="font-mono text-xs text-muted-foreground">{id}</p>
                {err && <p className="text-sm text-destructive">{err}</p>}
                <Button
                  variant="destructive"
                  disabled={state === "busy" || !token}
                  onClick={async () => {
                    setState("busy");
                    try {
                      await api(`/public/items/${id}/delete`, { body: { t: token } });
                      setState("done");
                    } catch (e) {
                      setErr((e as Error).message);
                      setState("ask");
                    }
                  }}
                >
                  Delete
                </Button>
              </>
            )}
          </CardContent>
        </Card>
      </main>
      <Footer />
    </div>
  );
}
