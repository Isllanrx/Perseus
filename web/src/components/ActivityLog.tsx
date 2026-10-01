import { ScrollText } from "lucide-react";
import { type ReactElement, useEffect, useRef } from "react";

import { useI18n } from "../i18n/context";
import { describeEvent } from "../i18n/describe";
import type { LogLine } from "../types";

interface ActivityLogProps {
  lines: LogLine[];
}

export function ActivityLog({ lines }: ActivityLogProps): ReactElement {
  const i18n = useI18n();
  const listRef = useRef<HTMLOListElement>(null);
  const lastLine = lines.at(-1);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const nearBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 80;
    if (nearBottom) list.scrollTop = list.scrollHeight;
  }, [lastLine?.id]);

  return (
    <details className="activity">
      <summary>
        <ScrollText size={15} aria-hidden />
        {i18n.t("activity.title")}
        {lastLine && <span className="activity-last">{describeEvent(i18n, lastLine.detail, lastLine.message)}</span>}
      </summary>
      {lines.length === 0 ? (
        <p className="activity-empty">{i18n.t("activity.empty")}</p>
      ) : (
        <ol ref={listRef} className="activity-list">
          {lines.map((line) => (
            <li key={line.id} data-level={line.level}>
              <time>{i18n.formatTime(line.ts, true)}</time>
              <span>{describeEvent(i18n, line.detail, line.message)}</span>
            </li>
          ))}
        </ol>
      )}
    </details>
  );
}
