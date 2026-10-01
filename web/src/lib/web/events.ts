export type CoreEvent =
  | { event: "planned"; album: string; artist: string; selected: number; total: number; target_dir: string }
  | { event: "resolve_failed"; url: string; reason: string }
  | { event: "track_progress"; track_id: number; bytes: number; fraction: number | null }
  | {
      event: "track_downloaded";
      track_id: number;
      file: string;
      bytes: number;
      attempts: number;
      elapsed_s: number;
      protocol: string;
    }
  | { event: "track_reused"; track_id: number; file: string }
  | { event: "playlist_file_written"; file: string; tracks: number }
  | {
      event: "track_retry";
      track_id: number;
      title: string;
      attempt: number;
      max: number;
      reason: string;
      delay_s: number;
    }
  | { event: "track_unavailable"; track_id: number; reason: string; reason_code: string | null }
  | { event: "track_failed"; track_id: number; reason: string; reason_code: string | null }
  | {
      event: "finished";
      downloaded: number;
      reused: number;
      failed: number;
      unavailable: number;
      bytes: number;
      elapsed_s: number;
    }
  | { event: "cancelled" }
  | { event: "archive_ready"; file: string; bytes: number };

export type Level = "DEBUG" | "INFO" | "WARNING" | "ERROR";

export function levelOf(event: CoreEvent): Level {
  switch (event.event) {
    case "track_progress":
      return "DEBUG";
    case "track_retry":
    case "track_unavailable":
    case "track_failed":
    case "cancelled":
      return "WARNING";
    case "resolve_failed":
      return "ERROR";
    default:
      return "INFO";
  }
}

export function messageOf(event: CoreEvent): string {
  switch (event.event) {
    case "planned":
      return `'${event.album}' por '${event.artist}': ${String(event.selected)} de ${String(event.total)} faixa(s) -> ${event.target_dir}`;
    case "resolve_failed":
      return `Falha ao resolver ${event.url}: ${event.reason}`;
    case "track_progress":
      return `Faixa ${String(event.track_id)}: ${String(event.bytes)} bytes`;
    case "track_downloaded":
      return `Concluido: ${event.file}`;
    case "track_reused":
      return `Ja existe: ${event.file}`;
    case "playlist_file_written":
      return `Playlist ${event.file} gravada com ${String(event.tracks)} faixa(s)`;
    case "track_retry":
      return `Tentativa ${String(event.attempt)}/${String(event.max)} falhou para '${event.title}': ${event.reason}. Nova tentativa em ${event.delay_s.toFixed(1)}s`;
    case "track_unavailable":
      return `Faixa ${String(event.track_id)} indisponivel: ${event.reason}`;
    case "track_failed":
      return `Faixa ${String(event.track_id)} falhou: ${event.reason}`;
    case "finished":
      return `Download finalizado: ${String(event.downloaded)} baixada(s), ${String(event.reused)} reaproveitada(s), ${String(event.failed)} com falha, ${String(event.unavailable)} indisponivel(is) em ${event.elapsed_s.toFixed(1)}s`;
    case "cancelled":
      return "Download cancelado pelo usuario";
    case "archive_ready":
      return `Arquivo pronto para salvar: ${event.file}`;
  }
}
