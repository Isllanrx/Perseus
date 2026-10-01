import { Ban, CircleAlert, CircleCheck, Eye, Languages, LoaderCircle, MonitorDown, TriangleAlert } from "lucide-react";
import { type ReactElement, type ReactNode, useId } from "react";

import mark from "../assets/perseus-mark.webp";
import { type I18n, useI18n } from "../i18n/context";
import { isLocale, LOCALES } from "../i18n/locales";
import { DESKTOP_DOWNLOAD_URL, IS_WEB } from "../lib/platform";
import type { Job } from "../types";

function StateIcon({ job }: { job: Job }): ReactNode {
  if (job.state === "running") {
    return job.mode === "watch" ? <Eye size={15} aria-hidden /> : <LoaderCircle size={15} className="spin" aria-hidden />;
  }
  if (job.state === "completed") return <CircleCheck size={15} aria-hidden />;
  if (job.state === "partial") return <TriangleAlert size={15} aria-hidden />;
  if (job.state === "cancelled") return <Ban size={15} aria-hidden />;
  return <CircleAlert size={15} aria-hidden />;
}

function jobSummary({ t }: I18n, job: Job): string {
  if (job.mode === "watch" && job.state === "running") return t("sidebar.watching");
  const done = job.counts.downloaded + job.counts.reused;
  if (job.state === "running") {
    return job.counts.selected ? t("sidebar.progress", { done, count: job.counts.selected }) : t("sidebar.preparing");
  }
  return t(`jobState.${job.state}`);
}

function LanguagePicker(): ReactElement {
  const { t, locale, setLocale } = useI18n();
  const id = useId();
  return (
    <div className="language">
      <label htmlFor={id} className="language-label">
        <Languages size={15} aria-hidden />
        <span>{t("sidebar.language")}</span>
      </label>
      <select
        id={id}
        className="language-select"
        value={locale}
        onChange={(event) => {
          if (isLocale(event.target.value)) setLocale(event.target.value);
        }}
      >
        {LOCALES.map((item) => (
          <option key={item.code} value={item.code} title={item.name} aria-label={item.name} lang={item.code}>
            {item.code.toUpperCase()}
          </option>
        ))}
      </select>
    </div>
  );
}

interface SidebarProps {
  version: string | undefined;
  jobs: Job[];
  selectedJobId: string | null;
  onSelect: (jobId: string) => void;
}

export function Sidebar({ version, jobs, selectedJobId, onSelect }: SidebarProps): ReactElement {
  const i18n = useI18n();
  const { t } = i18n;
  return (
    <aside className="sidebar">
      <div className="brand">
        <img src={mark} alt="" width={44} height={44} className="brand-mark" />
        <div>
          <p className="brand-name" lang="en" dir="ltr">
            Perseus
          </p>
          {version && <p className="brand-version">{t("sidebar.version", { version })}</p>}
        </div>
      </div>

      <nav aria-label={t("sidebar.nav")} className="history">
        <h2 className="history-title">{t("sidebar.title")}</h2>
        {jobs.length === 0 ? (
          <p className="history-empty">{t("sidebar.empty")}</p>
        ) : (
          <ul>
            {jobs.map((job) => (
              <li key={job.id}>
                <button
                  type="button"
                  className="history-item"
                  data-state={job.state}
                  aria-current={job.id === selectedJobId ? "true" : undefined}
                  onClick={() => onSelect(job.id)}
                >
                  <span className="history-icon">
                    <StateIcon job={job} />
                  </span>
                  <span className="history-text">
                    <span className="history-name">{job.title ?? job.url.replace(/^https:\/\/soundcloud\.com\//, "")}</span>
                    <span className="history-meta">
                      {jobSummary(i18n, job)}
                      <time dateTime={new Date(job.created_at * 1000).toISOString()}>{i18n.formatTime(job.created_at)}</time>
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </nav>

      <LanguagePicker />
      {IS_WEB && (
        <a className="desktop-link" href={DESKTOP_DOWNLOAD_URL} target="_blank" rel="noopener noreferrer">
          <MonitorDown size={15} aria-hidden />
          <span>{t("sidebar.desktopApp")}</span>
        </a>
      )}
      <p className="credit">{t("sidebar.credit")}</p>
    </aside>
  );
}
