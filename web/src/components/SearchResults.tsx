import { Disc3, ListMusic, Music, User } from "lucide-react";
import type { ReactElement, ReactNode } from "react";

import { useI18n } from "../i18n/context";
import { formatDuration } from "../lib/format";
import type { SearchHit, SearchHitKind } from "../types";

const ICONS: Record<SearchHitKind, ReactNode> = {
  track: <Music size={16} aria-hidden />,
  playlist: <ListMusic size={16} aria-hidden />,
  album: <Disc3 size={16} aria-hidden />,
  user: <User size={16} aria-hidden />,
};

interface SearchResultsProps {
  query: string;
  hits: SearchHit[];
  onPick: (hit: SearchHit) => void;
}

export function SearchResults({ query, hits, onPick }: SearchResultsProps): ReactElement {
  const { t } = useI18n();
  return (
    <section className="search" aria-labelledby="search-title">
      <h2 id="search-title" className="search-title">
        {t("search.title", { query })}
      </h2>
      {hits.length === 0 ? (
        <p className="search-empty">{t("search.empty", { query })}</p>
      ) : (
        <ul className="search-list">
          {hits.map((hit) => {
            const meta =
              hit.kind === "track"
                ? formatDuration(hit.duration_ms)
                : hit.track_count !== null
                  ? t("search.tracks", { count: hit.track_count })
                  : "";
            return (
              <li key={hit.url}>
                <button type="button" className="search-hit" onClick={() => onPick(hit)}>
                  {hit.artwork_url ? (
                    <img src={hit.artwork_url} alt="" width={44} height={44} referrerPolicy="no-referrer" />
                  ) : (
                    <span className="search-art" aria-hidden />
                  )}
                  <span className="search-text">
                    <span className="search-name">{hit.title}</span>
                    <span className="search-meta">
                      <span className="search-kind">
                        {ICONS[hit.kind]}
                        {t(`search.kind.${hit.kind}`)}
                      </span>
                      {hit.subtitle && <span>{hit.subtitle}</span>}
                      {meta && <span>{meta}</span>}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
