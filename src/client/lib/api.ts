export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

type ApiInit = Omit<RequestInit, "body" | "headers"> & {
  json?: unknown;
  body?: BodyInit | null;
  headers?: HeadersInit;
};

export async function api(path: string, init: ApiInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  let body = init.body;
  if (init.json !== undefined) {
    body = JSON.stringify(init.json);
    headers.set("content-type", "application/json");
  }
  const method = (init.method ?? "GET").toUpperCase();
  if (method !== "GET" && method !== "HEAD") headers.set("x-drop-request", "1");
  return fetch(path, {
    method,
    headers,
    body,
    credentials: "same-origin",
    cache: "no-store",
    signal: init.signal,
  });
}

export async function errorMessage(res: Response): Promise<string> {
  try {
    const data = (await res.json()) as { error?: unknown };
    if (typeof data.error === "string" && data.error.length > 0 && data.error.length < 400) {
      return data.error;
    }
  } catch {
    // The body was not JSON.
  }
  if (res.status === 413) return "That item is too large.";
  return "Something went wrong.";
}

export async function apiJson<T>(path: string, init?: ApiInit): Promise<T> {
  const res = await api(path, init);
  if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
  return (await res.json()) as T;
}

export async function apiBytes(path: string): Promise<Uint8Array> {
  const res = await api(path);
  if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
  return new Uint8Array(await res.arrayBuffer());
}

export function messageOf(error: unknown): string {
  if (error instanceof ApiError || error instanceof Error) return error.message;
  return "Something went wrong.";
}
