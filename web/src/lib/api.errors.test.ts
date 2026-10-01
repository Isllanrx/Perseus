/* eslint-disable @typescript-eslint/prefer-promise-reject-errors -- o IPC do Tauri rejeita com o valor serializado pelo backend, nao com Error */
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";

import { ApiError, api } from "./api";

async function rejectionFor(value: unknown): Promise<ApiError> {
  mockIPC(() => Promise.reject(value));
  try {
    await api.config();
  } catch (error) {
    return error as ApiError;
  }
  throw new Error("a chamada deveria falhar");
}

describe("api error mapping", () => {
  afterEach(() => {
    clearMocks();
  });

  it("keeps kind and code of a CommandError", async () => {
    const error = await rejectionFor({ kind: "invalid_input", code: "invalid_url", message: "Link invalido" });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.name).toBe("ApiError");
    expect([error.message, error.kind, error.code]).toEqual(["Link invalido", "invalid_input", "invalid_url"]);
  });

  it("falls back to internal kind and null code when those fields are not strings", async () => {
    const error = await rejectionFor({ kind: 42, code: 7, message: "falhou" });
    expect([error.message, error.kind, error.code]).toEqual(["falhou", "internal", null]);
  });

  it("ignores objects without a textual message", async () => {
    const error = await rejectionFor({ kind: "upstream", message: 500 });
    expect([error.kind, error.message]).toEqual(["ipc", "Sem comunicacao com o backend do Perseus."]);
  });

  it("wraps plain strings and rejects empty or missing values with the generic message", async () => {
    expect(await rejectionFor("canal fechado")).toMatchObject({ kind: "ipc", message: "canal fechado", code: null });
    for (const value of ["", null, undefined, 0]) {
      expect(await rejectionFor(value)).toMatchObject({ kind: "ipc", message: "Sem comunicacao com o backend do Perseus." });
    }
  });

  it("calls the expected backend commands", async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      return cmd === "list_jobs" ? [] : null;
    });
    await api.config();
    await api.jobs();
    await api.openFolder("job-1");
    expect(calls).toEqual([
      ["get_config", {}],
      ["list_jobs", {}],
      ["open_folder", { id: "job-1" }],
    ]);
  });
});
