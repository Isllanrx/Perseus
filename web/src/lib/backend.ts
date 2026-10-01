import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { JobEvent } from "../types";
import type { WebBackend } from "./web/backend";

const JOB_EVENT_CHANNEL = "perseus://job";

interface Backend {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  listen(handler: (event: JobEvent) => void): Promise<() => void>;
}

const tauriBackend: Backend = {
  invoke: (command, args) => invoke(command, args),
  listen: (handler) => listen<JobEvent>(JOB_EVENT_CHANNEL, (event) => handler(event.payload)),
};

function browserBackend(): Backend {
  let instance: Promise<WebBackend> | null = null;
  const load = (): Promise<WebBackend> =>
    (instance ??= import("./web/backend").then(({ WebBackend }) => new WebBackend()));
  return {
    invoke: async (command, args) => (await load()).invoke(command, args),
    listen: async (handler) => (await load()).listen(handler),
  };
}

export const backend: Backend = import.meta.env.MODE === "web" ? browserBackend() : tauriBackend;
