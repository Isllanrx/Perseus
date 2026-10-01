export class HttpError extends Error {
  constructor(
    readonly kind: string,
    readonly code: string,
    message: string,
    readonly status: number,
    readonly retryAfterS: number | null = null,
  ) {
    super(message);
    this.name = "HttpError";
  }
}

function isWireError(value: unknown): value is { kind: string; code: string; message: string } {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return typeof record.kind === "string" && typeof record.code === "string" && typeof record.message === "string";
}

async function parse<T>(response: Response): Promise<T> {
  const body: unknown = await response.json().catch(() => null);
  if (response.ok && body !== null) return body as T;
  const retryAfter = Number(response.headers.get("retry-after"));
  const retryAfterS = Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : null;
  if (isWireError(body)) throw new HttpError(body.kind, body.code, body.message, response.status, retryAfterS);
  throw new HttpError("upstream", "api", `Resposta inesperada do servidor (HTTP ${String(response.status)}).`, response.status, retryAfterS);
}

async function send<T>(input: string, init: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(input, { ...init, credentials: "same-origin", cache: "no-store" });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") throw error;
    throw new HttpError("network", "network", "Sem conexao com o servidor do Perseus.", 0);
  }
  return parse<T>(response);
}

export function getJson<T>(path: string, params: Record<string, string>, signal?: AbortSignal): Promise<T> {
  const query = new URLSearchParams(params).toString();
  return send<T>(query ? `${path}?${query}` : path, { method: "GET", ...(signal ? { signal } : {}) });
}

export function postJson<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
  return send<T>(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    ...(signal ? { signal } : {}),
  });
}
