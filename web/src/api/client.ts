import i18n from "../i18n";
import { useAuthStore } from "../store/auth";

/** Server errors arrive as `{"error": "<code>", "message": "<text>"}` (crates/yusr-server/src/error.rs). */
export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) {
    super(message);
  }
}

type Query = Record<string, string | number | boolean | null | undefined>;

function url(path: string, query?: Query): string {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query ?? {})) {
    if (v !== undefined && v !== null && v !== "") params.set(k, String(v));
  }
  const qs = params.toString();
  return `/api${path}${qs ? `?${qs}` : ""}`;
}

/** A fresh replay-protection key for one form submission (see `idempotent` in api.rs). */
export function newRequestKey(): string {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  // randomUUID needs a secure context; plain-HTTP LAN access falls back to getRandomValues.
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("");
}

async function send(method: string, path: string, body?: unknown, query?: Query, requestKey?: string): Promise<Response> {
  const token = useAuthStore.getState().token;
  const headers: Record<string, string> = {};
  if (requestKey) headers["Idempotency-Key"] = requestKey;
  if (token) headers.Authorization = `Bearer ${token}`;
  // FormData and files go as they are; the browser sets their content type.
  const raw = body instanceof FormData || body instanceof Blob;
  if (body !== undefined && !raw) headers["Content-Type"] = "application/json";
  let res: Response;
  try {
    res = await fetch(url(path, query), { method, headers, body: body === undefined ? undefined : raw ? body : JSON.stringify(body) });
  } catch {
    throw new ApiError(0, "network", "network error");
  }
  if (res.status === 401 && path !== "/auth/login") {
    useAuthStore.getState().logout();
  }
  if (!res.ok) {
    let code = "http_error";
    let message = res.statusText || `HTTP ${res.status}`;
    // axum's own rejections (malformed JSON, missing field) are plain text, not {error, message}.
    const body = await res.text().catch(() => "");
    try {
      const data = JSON.parse(body);
      code = data.error ?? code;
      message = data.message ?? message;
    } catch {
      if (body) message = body.slice(0, 300);
    }
    throw new ApiError(res.status, code, message);
  }
  return res;
}

async function json<T>(method: string, path: string, body?: unknown, query?: Query, requestKey?: string): Promise<T> {
  const res = await send(method, path, body, query, requestKey);
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}

export const api = {
  get: <T>(path: string, query?: Query) => json<T>("GET", path, undefined, query),
  /** `requestKey` makes the POST safe to retry: the server runs it at most once per key. */
  post: <T>(path: string, body: unknown, requestKey?: string) => json<T>("POST", path, body, undefined, requestKey),
  put: <T>(path: string, body: unknown) => json<T>("PUT", path, body),
  patch: <T>(path: string, body: unknown) => json<T>("PATCH", path, body),
  /** Authenticated non-JSON download (receipt HTML, invoice XML). */
  text: async (path: string, query?: Query) => (await send("GET", path, undefined, query)).text(),
  /** The response itself, for files, streams and multipart uploads; errors still throw ApiError. */
  raw: (method: string, path: string, body?: FormData | Blob | object) => send(method, path, body),
};

/** Codes whose server message carries the specifics (which field, which rule); shown after the translation. */
const DETAILED = new Set(["invalid", "bad_request", "zatca", "conflict", "constraint_violation", "gateway_error", "http_error"]);

/** User-facing text in the active language, keyed by the server's error code. */
export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const key = `errors.${e.code}`;
    if (!i18n.exists(key)) return e.message || i18n.t("errors.unexpected");
    const text = i18n.t(key);
    return DETAILED.has(e.code) && e.message ? `${text}: ${e.message}` : text;
  }
  return i18n.t("errors.unexpected");
}

/** Opens server-rendered HTML (e.g. a donation receipt) in a new tab for printing. */
export function openHtml(html: string) {
  const blobUrl = URL.createObjectURL(new Blob([html], { type: "text/html;charset=utf-8" }));
  window.open(blobUrl, "_blank", "noopener");
  setTimeout(() => URL.revokeObjectURL(blobUrl), 60_000);
}

export function download(filename: string, content: string, type: string) {
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([content], { type }));
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}
