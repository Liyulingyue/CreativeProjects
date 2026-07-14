let apiBase = import.meta.env.DEV ? "/api" : "http://127.0.0.1:8001/api";
let apiBaseInitialized = false;
let invokeFn: (<T>(cmd: string, args?: Record<string, unknown>) => Promise<T>) | null = null;

type InProcessResponse = [number, string, string | null];

export async function initApiBase(): Promise<void> {
  if (apiBaseInitialized) return;
  apiBaseInitialized = true;

  if (import.meta.env.DEV) {
    return;
  }

  const tauriWindow = typeof window !== "undefined" ? (window as { __TAURI_INTERNALS__?: unknown }) : undefined;
  if (!tauriWindow?.__TAURI_INTERNALS__) {
    return;
  }

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    invokeFn = invoke;
    const runtimeApiBase = await invoke<string>("get_api_base");
    if (runtimeApiBase === "inproc") {
      apiBase = "inproc";
      return;
    }
    if (runtimeApiBase && (runtimeApiBase.startsWith("http://") || runtimeApiBase.startsWith("https://"))) {
      apiBase = runtimeApiBase;
    }
  } catch {
    // Keep fallback API base when command is unavailable.
  }
}

function isInProcessApi(): boolean {
  return apiBase === "inproc";
}

async function getInvoke() {
  if (invokeFn) return invokeFn;
  const { invoke } = await import("@tauri-apps/api/core");
  invokeFn = invoke;
  return invoke;
}

function apiOrigin(): string {
  if (apiBase.startsWith("http://") || apiBase.startsWith("https://")) {
    return apiBase.endsWith("/api") ? apiBase.slice(0, -4) : apiBase;
  }
  return "";
}

async function request<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  if (isInProcessApi()) {
    const [path, query] = endpoint.split("?", 2);
    const method = options.method ?? "GET";
    const body = typeof options.body === "string" ? options.body : undefined;
    const invoke = await getInvoke();
    const [status, bodyText] = await invoke<InProcessResponse>("api_request", {
      method,
      path: `/api${path}`,
      query: query ?? null,
      body: body ?? null,
    });

    if (status < 200 || status >= 300) {
      const error = (() => {
        try {
          return JSON.parse(bodyText || "{}");
        } catch {
          return { detail: bodyText || `HTTP ${status}` };
        }
      })();
      throw new Error(error.detail || `HTTP ${status}`);
    }

    return JSON.parse(bodyText || "null") as T;
  }

  const url = `${apiBase}${endpoint}`;
  const response = await fetch(url, {
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
    ...options,
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ detail: response.statusText }));
    throw new Error(error.detail || `HTTP ${response.status}`);
  }

  return response.json();
}

export function apiUrl(path: string): string {
  if (isInProcessApi()) {
    return `/api${path}`;
  }
  return `${apiBase}${path}`;
}

export function resolveApiUrl(pathOrUrl: string): string {
  if (!pathOrUrl) return pathOrUrl;
  if (pathOrUrl.startsWith("http://") || pathOrUrl.startsWith("https://")) {
    return pathOrUrl;
  }
  if (pathOrUrl.startsWith("/api")) {
    const origin = apiOrigin();
    return origin ? `${origin}${pathOrUrl}` : pathOrUrl;
  }
  return pathOrUrl;
}

function decodePathFromThumbnailUrl(thumbnailUrl: string): string | null {
  const match = thumbnailUrl.match(/[?&]path=([^&]+)/);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return null;
  }
}

export async function getThumbnailSrc(path: string, full = false): Promise<string> {
  if (isInProcessApi()) {
    const invoke = await getInvoke();
    return invoke<string>("get_thumbnail_data_url", { path, full });
  }
  return apiUrl(`/thumbnails?path=${encodeURIComponent(path)}${full ? "&full=1" : ""}`);
}

export async function resolveThumbnailUrl(thumbnailUrl: string, path?: string): Promise<string> {
  if (isInProcessApi()) {
    const resolvedPath = path ?? decodePathFromThumbnailUrl(thumbnailUrl);
    if (!resolvedPath) return "";
    return getThumbnailSrc(resolvedPath, false);
  }
  return resolveApiUrl(thumbnailUrl);
}

export { request, isInProcessApi };
