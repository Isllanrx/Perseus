import { useEffect, useLayoutEffect, useRef } from "react";

import type { Job, JobEvent, LogLine, TrackEvent } from "../types";

export interface WatchStatus {
  message: string;
  detail?: Record<string, unknown> | null;
}
import { api } from "./api";
import { backend } from "./backend";

export interface JobEventHandlers {
  onLog: (jobId: string, line: LogLine) => void;
  onTrack: (jobId: string, event: TrackEvent) => void;
  onState: (job: Job) => void;
  onWatch: (jobId: string, status: WatchStatus) => void;
  onEnd: (jobId: string) => void;
}

export function useJobEvents(knownJobIds: string[], handlers: JobEventHandlers): void {
  const handlersRef = useRef(handlers);
  useLayoutEffect(() => {
    handlersRef.current = handlers;
  });
  const seen = useRef(new Map<string, Set<number>>());
  const latestState = useRef(new Map<string, number>());

  const dispatch = useRef((item: JobEvent): void => {
    let ids = seen.current.get(item.job_id);
    if (!ids) {
      ids = new Set();
      seen.current.set(item.job_id, ids);
    }
    if (ids.has(item.id)) return;
    ids.add(item.id);
    const current = handlersRef.current;
    switch (item.event) {
      case "log":
        current.onLog(item.job_id, { ...(item.data as Omit<LogLine, "id">), id: item.id });
        break;
      case "track":
        current.onTrack(item.job_id, item.data as TrackEvent);
        break;
      case "state":
        if (item.id > (latestState.current.get(item.job_id) ?? 0)) {
          latestState.current.set(item.job_id, item.id);
          current.onState(item.data as Job);
        }
        break;
      case "watch":
        current.onWatch(item.job_id, item.data as WatchStatus);
        break;
      case "end":
        current.onEnd(item.job_id);
        break;
    }
  });

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    backend.listen((item) => dispatch.current(item)).then(
      (stop) => {
        if (disposed) stop();
        else unlisten = stop;
      },
      () => undefined,
    );
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const replayed = useRef(new Set<string>());
  const replayKey = knownJobIds.join(",");
  useEffect(() => {
    for (const id of replayKey ? replayKey.split(",") : []) {
      if (replayed.current.has(id)) continue;
      replayed.current.add(id);
      api.jobEvents(id, 0).then(
        (items) => {
          for (const item of items) dispatch.current(item);
        },
        () => undefined,
      );
    }
  }, [replayKey]);
}
