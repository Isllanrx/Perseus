import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { describeEvent } from "./i18n/describe";
import { LOCALES } from "./i18n/locales";
import { createTranslator } from "./i18n/translate";
import type { CreateJobBody } from "./lib/api";

function contract(name: string): unknown {
  return JSON.parse(readFileSync(resolve(process.cwd(), "..", "contracts", name), "utf8"));
}

type Detail = Record<string, unknown> & { event: string };

const FALLBACK = "\u0000sem-traducao";

describe("contract: log events emitted by the backend", () => {
  const details = contract("log-events.json") as Detail[];

  it("covers every event the backend logs", () => {
    expect(details.length).toBeGreaterThanOrEqual(20);
    expect(new Set(details.map((d) => d.event)).size).toBe(details.length);
  });

  it.each(LOCALES.map((locale) => locale.code))("renders every event in %s without placeholders", (code) => {
    const tr = createTranslator(code);
    for (const detail of details) {
      const line = describeEvent(tr, detail, FALLBACK);
      expect(line, `${code}: ${detail.event} sem traducao`).not.toBe(FALLBACK);
      expect(line, `${code}: ${detail.event} com variavel nao preenchida`).not.toMatch(/\{[a-z_]+\}/);
      expect(line.trim().length, `${code}: ${detail.event} vazio`).toBeGreaterThan(0);
    }
  });
});

describe("contract: job request accepted by the backend", () => {
  it("uses exactly the fields the backend knows", () => {
    const fixture = contract("job-in.json") as Record<string, unknown>;
    const sample: Required<CreateJobBody> = {
      url: "",
      mode: "playlist",
      output_dir: null,
      workers: 1,
      limit: null,
      interval: 60,
      quality: "compatible",
      name_template: null,
      min_duration_s: null,
      max_duration_s: null,
      max_kbps: null,
      write_playlist_file: true,
      original_artwork: false,
      sync_removed: false,
      use_library: true,
    };
    expect(Object.keys(sample).sort()).toEqual(Object.keys(fixture).sort());
  });
});
