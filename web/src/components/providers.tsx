"use client";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider } from "next-themes";
import { useEffect, useState } from "react";

import { toast } from "sonner";

import { Toaster } from "@/components/ui/sonner";
import { refreshSoon } from "@/lib/cache";
import { queue } from "@/lib/upload";

export function Providers({ children }: { children: React.ReactNode }) {
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: { queries: { refetchOnWindowFocus: false, staleTime: 15_000, retry: 1 } },
      }),
  );
  useEffect(() => {
    queue.onDone = (item) => {
      if (document.hasFocus()) navigator.clipboard?.writeText(item.url).then(() => toast.success("Link copied"), () => {});
      refreshSoon(client);
    };
  }, [client]);
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <QueryClientProvider client={client}>
        {children}
        {/* bottom right, out of the way; on phones above the bottom navigation */}
        <Toaster richColors closeButton position="bottom-right" mobileOffset={{ bottom: "calc(3.5rem + env(safe-area-inset-bottom))" }} />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
