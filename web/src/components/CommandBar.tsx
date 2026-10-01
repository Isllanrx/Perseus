import { ClipboardPaste, Download, Eye, LoaderCircle, Search, Square } from "lucide-react";
import { type ReactElement, type SyntheticEvent, useId } from "react";

import { useI18n } from "../i18n/context";
import { IS_WEB } from "../lib/platform";
import type { Mode } from "../types";

const MODES = [
  { value: "playlist", label: "mode.playlist", hint: "mode.playlistHint" },
  { value: "track", label: "mode.track", hint: "mode.trackHint" },
  { value: "watch", label: "mode.watch", hint: "mode.watchHint" },
] as const satisfies readonly { value: Mode; label: string; hint: string }[];

const AVAILABLE_MODES = IS_WEB ? MODES.filter((option) => option.value !== "watch") : MODES;

export interface CommandError {
  text: string;
  detail?: string | null;
}

interface CommandBarProps {
  url: string;
  mode: Mode;
  isQuery: boolean;
  inspecting: boolean;
  running: boolean;
  error: CommandError | null;
  onUrlChange: (url: string) => void;
  onModeChange: (mode: Mode) => void;
  onInspect: () => void;
  onStart: () => void;
  onCancel: () => void;
}

export function CommandBar(props: CommandBarProps): ReactElement {
  const { url, mode, isQuery, inspecting, running, error, onUrlChange, onModeChange, onInspect, onStart, onCancel } =
    props;
  const { t } = useI18n();
  const inputId = useId();
  const errorId = useId();

  async function paste(): Promise<void> {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (text) onUrlChange(text);
    } catch {
      document.getElementById(inputId)?.focus();
    }
  }

  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!running) onStart();
  }

  return (
    <form className="command" onSubmit={submit} noValidate>
      <label htmlFor={inputId} className="command-label">
        {t("command.label")}
      </label>
      <div className="command-row">
        <input
          id={inputId}
          className="command-input"
          type="text"
          inputMode="url"
          dir="ltr"
          autoComplete="off"
          spellCheck={false}
          placeholder={t("command.placeholder")}
          value={url}
          onChange={(event) => onUrlChange(event.target.value)}
          onBlur={() => {
            if (url.trim() && !isQuery) onInspect();
          }}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? errorId : undefined}
        />
        <button type="button" className="icon-button" onClick={() => void paste()} title={t("command.pasteTitle")}>
          <ClipboardPaste size={18} aria-hidden />
          <span className="sr-only">{t("command.paste")}</span>
        </button>
        <button type="button" className="ghost-button" onClick={onInspect} disabled={!url.trim() || inspecting}>
          {inspecting ? <LoaderCircle size={16} className="spin" aria-hidden /> : <Search size={16} aria-hidden />}
          <span className="command-inspect-label">{isQuery ? t("command.search") : t("command.inspect")}</span>
        </button>
      </div>
      {error && (
        <p id={errorId} className="command-error" role="alert" title={error.detail ?? undefined}>
          {error.text}
        </p>
      )}

      <div className="command-actions">
        <fieldset className="modes">
          <legend className="sr-only">{t("command.modesLegend")}</legend>
          {AVAILABLE_MODES.map((option) => (
            <label key={option.value} className="mode" title={t(option.hint)}>
              <input
                type="radio"
                name="mode"
                value={option.value}
                checked={mode === option.value}
                onChange={() => onModeChange(option.value)}
              />
              <span>{t(option.label)}</span>
            </label>
          ))}
        </fieldset>

        {running ? (
          <button type="button" className="primary-button is-cancel" onClick={onCancel}>
            <Square size={15} aria-hidden />
            {t("command.cancel")}
          </button>
        ) : (
          <button type="submit" className="primary-button" disabled={!url.trim()}>
            {mode === "watch" ? <Eye size={17} aria-hidden /> : <Download size={17} aria-hidden />}
            {mode === "watch" ? t("command.watch") : t("command.download")}
          </button>
        )}
      </div>
    </form>
  );
}
