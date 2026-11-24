"use client";

import { useQuery } from "@tanstack/react-query";

import { api, type Instance } from "@/lib/api";

export function useInstance() {
  return useQuery({ queryKey: ["instance"], queryFn: () => api<Instance>("/instance"), staleTime: 60_000 });
}
