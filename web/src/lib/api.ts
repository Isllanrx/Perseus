import type { AppConfig, Job, JobEvent, Mode, Quality, Release, SearchHit } from "../types";
import { backend } from "./backend";

export type ApiErrorKind =
  | "invalid_input"
  | "not_found"
  | "upstream"
  | "unavailable"
  | "download"
  | "cancelled"
  | "too_many_jobs"
  | "job_not_found"
  | "conflict"
  | "internal"
  | "rate_limited"
  | "network"
  | "ipc";

export class ApiError extends Error {
  constructor(
    message: string,
    readonly kind: ApiErrorKind,
    readonly code: string | null = null,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

function toApiError(error: unknown): ApiError {
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
    const kind = "kind" in error && typeof error.kind === "string" ? (error.kind as ApiErrorKind) : "internal";
    const code = "code" in error && typeof error.code === "string" ? error.code : null;
    return new ApiError(error.message, kind, code);
  }
  if (typeof error === "string" && error) return new ApiError(error, "ipc");
  return new ApiError("Sem comunicacao com o backend do Perseus.", "ipc");
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return (await backend.invoke(command, args)) as T;
  } catch (error) {
    throw toApiError(error);
  }
}

export interface CreateJobBody {
  url: string;
  mode: Mode;
  output_dir: string | null;
  workers: number;
  limit: number | null;
  interval: number;
  quality: Quality;
  name_template: string | null;
  min_duration_s: number | null;
  max_duration_s: number | null;
  max_kbps: number | null;
  write_playlist_file: boolean;
  original_artwork: boolean;
  sync_removed: boolean;
  use_library: boolean;
}

export const api = {
  config: (): Promise<AppConfig> => call<AppConfig>("get_config"),
  inspect: (url: string, mode: Mode): Promise<Release> => call<Release>("inspect", { url, mode }),
  search: (query: string): Promise<SearchHit[]> => call<SearchHit[]>("search", { query }),
  jobs: (): Promise<Job[]> => call<Job[]>("list_jobs"),
  jobEvents: (id: string, after: number): Promise<JobEvent[]> => call<JobEvent[]>("job_events", { id, after }),
  createJob: (spec: CreateJobBody): Promise<Job> => call<Job>("create_job", { spec }),
  cancelJob: (id: string): Promise<Job> => call<Job>("cancel_job", { id }),
  openFolder: (id: string): Promise<void> => call<undefined>("open_folder", { id }),
  pickOutputDir: (title: string, initial: string | null): Promise<string | null> =>
    call<string | null>("pick_output_dir", { title, initial }),
};
