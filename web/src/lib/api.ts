import { useSyncExternalStore } from "react";

export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
  ) {
    super(message);
  }
}

export async function api<T = unknown>(path: string, init: { method?: string; body?: unknown; signal?: AbortSignal } = {}): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method: init.method ?? (init.body === undefined ? "GET" : "POST"),
    headers: init.body === undefined ? undefined : { "Content-Type": "application/json" },
    body: init.body === undefined ? undefined : JSON.stringify(init.body),
    credentials: "same-origin",
    signal: init.signal,
  });
  if (res.status === 204) return undefined as T;
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(data.error ?? `Request failed (${res.status})`, res.status);
  return data as T;
}

export type User = {
  id: number;
  username: string;
  role: "admin" | "user";
  avatar_url: string | null;
  twitch: boolean;
  has_password: boolean;
  quota_mb: number | null;
  used_bytes: number;
  disabled: boolean;
  created_at: number;
  /** saw or skipped the welcome tour */
  onboarded: boolean;
  /** made the instance: other admins can't change or remove this account */
  owner: boolean;
};

export type Instance = {
  name: string;
  url: string;
  setup: boolean;
  registration: "closed" | "invite" | "open";
  twitch: boolean;
  max_upload: number;
  chunk_size: number;
  version: string;
  user: User | null;
};

export type ViewType = "image" | "video" | "audio" | "pdf" | "text" | "link" | "other";

export type Item = {
  id: string;
  kind: "file" | "paste" | "link";
  name: string;
  mime: string;
  type: ViewType;
  size: number;
  width: number | null;
  height: number | null;
  duration: number | null;
  language: string | null;
  target?: string;
  thumb: boolean;
  locked: boolean;
  expires_at: number | null;
  burn: boolean;
  max_views: number | null;
  views: number;
  album_id: string | null;
  created_at: number;
  url: string;
  raw_url: string;
  thumb_url: string | null;
  download_url: string;
  deletion_url?: string;
  uploader?: { name: string; avatar_url: string | null } | null;
};

/** What the public item page gets (locked items only show a few fields). */
export type PublicItem = Partial<Item> &
  Pick<Item, "id" | "kind" | "name" | "type" | "locked" | "created_at"> & {
    owner: boolean;
    unlocked: boolean;
    content?: string;
  };

export type Album = {
  id: string;
  title: string;
  description: string;
  public: boolean;
  created_at: number;
  count: number;
  size: number;
  covers: string[];
  url: string;
};

export type PublicAlbum = {
  id: string;
  title: string;
  description: string;
  public: boolean;
  created_at: number;
  items: Item[];
  owner: boolean;
  uploader: { name: string; avatar_url: string | null } | null;
  url: string;
};

export type Settings = {
  registration: "closed" | "invite" | "open";
  max_upload_mb: number;
  default_quota_mb: number;
  twitch_client_id: string;
  /** only sent when changing it; the server never returns it */
  twitch_client_secret: string;
  twitch_secret_set?: boolean;
  twitch_from_env?: boolean;
  twitch_redirect_url?: string;
};

export type Version = { current: string; latest: string | null; update: boolean; repo: string; docker: boolean };

/** Upload options as the API takes them. */
export type UploadOptions = {
  expires?: string;
  password?: string;
  burn?: boolean;
  max_views?: number;
  album?: string;
};

/** Data the server put into the page (item and album pages). */
export function pageData<T>(): T | null {
  if (typeof document === "undefined") return null;
  const el = document.getElementById("__DATA__");
  if (!el?.textContent) return null;
  try {
    return JSON.parse(el.textContent) as T;
  } catch {
    return null;
  }
}

let cachedPageData: unknown = undefined;
const noop = () => () => {};

/** `pageData()` as a stable hook value (null during the static render, so hydration matches). */
export function usePageData<T>(): T | null {
  return useSyncExternalStore(
    noop,
    () => {
      if (cachedPageData === undefined) cachedPageData = pageData<T>();
      return cachedPageData as T | null;
    },
    () => null,
  );
}
