import type { Translator } from "./translate";
import type { Vars } from "./types";

type Detail = Record<string, unknown>;

const ERROR_ALIASES: Record<string, string> = {
  untrusted_url: "invalid_url",
};

function isDetail(value: unknown): value is Detail {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function text(value: unknown): string | null {
  return typeof value === "string" && value ? value : null;
}

export function reasonText(tr: Translator, reason: unknown, code: unknown): string | null {
  const key = `reason.${String(code)}`;
  if (text(code) && tr.has(key)) return tr.t(key);
  return text(reason);
}

export function specificErrorText(tr: Translator, code: unknown): string | null {
  const raw = text(code);
  if (!raw) return null;
  const key = `error.${ERROR_ALIASES[raw] ?? raw}`;
  return tr.has(key) ? tr.t(key) : null;
}

export function errorText(tr: Translator, code: unknown): string {
  return specificErrorText(tr, code) ?? tr.t("error.generic");
}

export function describeEvent(tr: Translator, detail: unknown, fallback: string): string {
  if (!isDetail(detail)) return fallback;
  const key = `log.${String(detail.event)}`;
  if (!tr.has(key)) return fallback;
  const vars: Vars = {};
  for (const [name, value] of Object.entries(detail)) {
    if (typeof value === "string" || typeof value === "number") vars[name] = value;
  }
  const reason =
    detail.event === "job_failed"
      ? (specificErrorText(tr, detail.code) ?? text(detail.reason))
      : reasonText(tr, detail.reason, detail.reason_code);
  if (reason) vars.reason = reason;
  return tr.t(key, vars);
}
