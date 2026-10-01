import { describe, expect, it } from "vitest";

import { formatDuration, formatTotalDuration } from "./format";

describe("formatDuration", () => {
  it.each([
    [null, "--:--"],
    [0, "--:--"],
    [-5, "--:--"],
    [61_000, "1:01"],
    [599_999, "9:59"],
    [3_723_000, "1:02:03"],
  ])("%s ms -> %s", (input, expected) => {
    expect(formatDuration(input)).toBe(expected);
  });
});

describe("formatTotalDuration", () => {
  it.each([
    [59 * 60_000, "59 min"],
    [60 * 60_000, "1 hr"],
    [135 * 60_000, "2 hr 15 min"],
  ])("%s ms -> %s (en)", (input, expected) => {
    expect(formatTotalDuration(input, "en")).toBe(expected);
  });

  it("uses the units of the active language", () => {
    expect(formatTotalDuration(135 * 60_000, "pt-BR")).toBe("2 h 15 min");
    expect(formatTotalDuration(45 * 60_000, "ru")).toMatch(/^45 мин/);
  });
});
