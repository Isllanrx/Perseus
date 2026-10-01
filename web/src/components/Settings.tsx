import { FolderSearch, SlidersHorizontal } from "lucide-react";
import { type ReactElement, useId } from "react";

import { useI18n } from "../i18n/context";
import type { MessageKey } from "../i18n/messages/pt-BR";
import { IS_WEB, canPickFolder } from "../lib/platform";
import type { AppConfig, JobOptions, Mode, Quality } from "../types";

interface SettingsProps {
  config: AppConfig | null;
  options: JobOptions;
  mode: Mode;
  onChange: (options: JobOptions) => void;
  onPickFolder: () => void;
}

type BooleanOption = "askFolder" | "writePlaylistFile" | "originalArtwork" | "syncRemoved" | "useLibrary";

const TOGGLES: readonly { key: Exclude<BooleanOption, "askFolder">; label: MessageKey }[] = [
  { key: "useLibrary", label: "settings.useLibrary" },
  { key: "writePlaylistFile", label: "settings.writePlaylist" },
  { key: "originalArtwork", label: "settings.originalArtwork" },
  { key: "syncRemoved", label: "settings.syncRemoved" },
];

const DESKTOP_ONLY: ReadonlySet<BooleanOption> = new Set(["useLibrary", "syncRemoved"]);
const AVAILABLE_TOGGLES = IS_WEB ? TOGGLES.filter(({ key }) => !DESKTOP_ONLY.has(key)) : TOGGLES;

function optionalNumber(raw: string, min: number): number | null {
  return raw ? Math.max(min, Number(raw)) : null;
}

export function Settings({ config, options, mode, onChange, onPickFolder }: SettingsProps): ReactElement {
  const { t, formatNumber } = useI18n();
  const ids = {
    dir: useId(),
    ask: useId(),
    workers: useId(),
    limit: useId(),
    interval: useId(),
    quality: useId(),
    template: useId(),
    templateHint: useId(),
    minDuration: useId(),
    maxDuration: useId(),
    maxKbps: useId(),
  };
  const set = <K extends keyof JobOptions>(key: K, value: JobOptions[K]): void => {
    onChange({ ...options, [key]: value });
  };
  const toggleId = useId();

  return (
    <details className="settings">
      <summary>
        <SlidersHorizontal size={15} aria-hidden />
        {t("settings.title")}
        <span className="settings-peek">
          {t("settings.peekWorkers", { count: options.workers })}
          {options.limit ? t("settings.peekLimit", { count: options.limit }) : ""}
        </span>
      </summary>

      <div className="settings-grid">
        {IS_WEB ? (
          <div className="field field-wide">
            {canPickFolder() ? (
              <>
                <label htmlFor={ids.ask} className="field-check">
                  <input
                    id={ids.ask}
                    type="checkbox"
                    checked={options.askFolder}
                    onChange={(event) => set("askFolder", event.target.checked)}
                  />
                  {t("settings.saveToFolder")}
                </label>
                <p className="field-hint">{t("settings.saveToFolderHint")}</p>
              </>
            ) : (
              <p className="field-hint">{t("settings.browserDownloadHint")}</p>
            )}
          </div>
        ) : (
          <div className="field field-wide">
            <label htmlFor={ids.dir}>{t("settings.outputDir")}</label>
            <div className="field-row">
              <input
                id={ids.dir}
                type="text"
                dir="ltr"
                spellCheck={false}
                value={options.outputDir}
                placeholder={config?.default_output_dir ?? t("settings.outputDirPlaceholder")}
                onChange={(event) => set("outputDir", event.target.value)}
              />
              <button type="button" className="ghost-button" onClick={onPickFolder}>
                <FolderSearch size={16} aria-hidden />
                {t("settings.pickFolder")}
              </button>
            </div>
            <p className="field-hint">{t("settings.outputDirHint")}</p>
            <label htmlFor={ids.ask} className="field-check">
              <input
                id={ids.ask}
                type="checkbox"
                checked={options.askFolder}
                onChange={(event) => set("askFolder", event.target.checked)}
              />
              {t("settings.askFolder")}
            </label>
          </div>

        )}

        <div className="field">
          <label htmlFor={ids.workers}>
            {t("settings.workers")} <output htmlFor={ids.workers}>{formatNumber(options.workers)}</output>
          </label>
          <input
            id={ids.workers}
            type="range"
            min={1}
            max={config?.max_workers ?? 16}
            value={options.workers}
            onChange={(event) => set("workers", Number(event.target.value))}
          />
        </div>

        <div className="field">
          <label htmlFor={ids.quality}>{t("settings.quality")}</label>
          <select
            id={ids.quality}
            value={options.quality}
            onChange={(event) => set("quality", event.target.value as Quality)}
          >
            <option value="compatible">{t("settings.qualityCompatible")}</option>
            <option value="best">{t("settings.qualityBest")}</option>
          </select>
        </div>

        <div className="field">
          <label htmlFor={ids.limit}>{t("settings.limit")}</label>
          <input
            id={ids.limit}
            type="number"
            min={1}
            max={10000}
            placeholder={t("settings.limitPlaceholder")}
            value={options.limit ?? ""}
            onChange={(event) => set("limit", optionalNumber(event.target.value, 1))}
          />
        </div>

        {mode === "watch" && (
          <div className="field">
            <label htmlFor={ids.interval}>{t("settings.interval")}</label>
            <input
              id={ids.interval}
              type="number"
              min={config?.interval_min ?? 10}
              max={config?.interval_max ?? 3600}
              value={options.interval}
              onChange={(event) => set("interval", Number(event.target.value))}
            />
          </div>
        )}

        <div className="field">
          <label htmlFor={ids.minDuration}>{t("settings.minDuration")}</label>
          <input
            id={ids.minDuration}
            type="number"
            min={0}
            placeholder={t("settings.noLimit")}
            value={options.minDurationS ?? ""}
            onChange={(event) => set("minDurationS", optionalNumber(event.target.value, 0))}
          />
        </div>

        <div className="field">
          <label htmlFor={ids.maxDuration}>{t("settings.maxDuration")}</label>
          <input
            id={ids.maxDuration}
            type="number"
            min={1}
            placeholder={t("settings.noLimit")}
            value={options.maxDurationS ?? ""}
            onChange={(event) => set("maxDurationS", optionalNumber(event.target.value, 1))}
          />
        </div>

        {!IS_WEB && (
          <div className="field">
            <label htmlFor={ids.maxKbps}>{t("settings.maxKbps")}</label>
            <input
              id={ids.maxKbps}
              type="number"
              min={64}
              step={64}
              placeholder={t("settings.noLimit")}
              value={options.maxKbps ?? ""}
              onChange={(event) => set("maxKbps", optionalNumber(event.target.value, 64))}
            />
          </div>
        )}

        <div className="field field-wide">
          <label htmlFor={ids.template}>{t("settings.nameTemplate")}</label>
          <input
            id={ids.template}
            type="text"
            dir="ltr"
            spellCheck={false}
            value={options.nameTemplate}
            placeholder="{number}. {artist} - {title}"
            aria-describedby={ids.templateHint}
            onChange={(event) => set("nameTemplate", event.target.value)}
          />
          <p id={ids.templateHint} className="field-hint">
            {t("settings.nameTemplateHint")}
          </p>
        </div>

        <div role="group" className="field field-wide field-toggles" aria-label={t("settings.title")}>
          {AVAILABLE_TOGGLES.map(({ key, label }) => (
            <label key={key} htmlFor={`${toggleId}-${key}`} className="field-check">
              <input
                id={`${toggleId}-${key}`}
                type="checkbox"
                checked={options[key]}
                onChange={(event) => set(key, event.target.checked)}
              />
              {t(label)}
            </label>
          ))}
        </div>
      </div>
    </details>
  );
}
