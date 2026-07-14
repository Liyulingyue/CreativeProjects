let apiBase = import.meta.env.DEV ? "/api" : "http://127.0.0.1:8001/api";
let apiBaseInitialized = false;

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
    const runtimeApiBase = await invoke<string>("get_api_base");
    if (runtimeApiBase && (runtimeApiBase.startsWith("http://") || runtimeApiBase.startsWith("https://"))) {
      apiBase = runtimeApiBase;
    }
  } catch {
    // Keep fallback API base when command is unavailable.
  }
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

export { request };
