# Backlog priorizado

Origem principal: analise comparativa com scdl-org/scdl, yt-dlp, yaaaarn/downcloud, imthaghost/scdl,
jerry08/SoundCloudExplode e NotTobi/soundcloud-dl (2026-09-29). Esforco: P (<1 dia), M (1-3 dias), G (>3 dias).

| # | Item | Prioridade | Esforco | Por que |
|---|---|---|---|---|
| 1 | ~~Download do arquivo original~~ | Descartado | — | `/tracks/{id}/download` retorna 401 sem login; o produto e anonimo. |
| 2 | ~~Perfis de usuario: uploads, likes, reposts, playlists~~ Concluido v1.0 | Alta | M | Hoje a URL de perfil retorna "recurso nao suportado"; scdl e downcloud baixam catalogos inteiros. Nova fatia `features/profile.rs` reaproveitando `download`. |
| 3 | ~~Archive por track ID + modo sync~~ Concluido v1.0 (sync move para `Removed/`, nunca apaga) | Alta | M | Reaproveitamento atual depende do nome do arquivo; renomear titulo quebra. Archive por ID (arquivo local) torna idempotencia robusta; sync remove faixas saidas da playlist (opt-in, destrutivo — exigir confirmacao). |
| 4 | ~~Templates de nome de arquivo~~ Concluido v1.0 | Media | P | Pedido comum (scdl `--name-format`). Validar placeholders e manter `sanitize_filename` + `ensure_within`. |
| 5 | ~~HLS com segmentos em paralelo~~ | Concluido v1.0 | — | 6 segmentos por faixa, gravados em ordem, retry por segmento. |
| 6 | ~~Geracao de `.m3u` por playlist~~ Concluido v1.0 (`.m3u8`) | Media | P | scdl gera; facilita importar em players. |
| 7 | Remux opcional com ffmpeg (MP4 fragmentado de HLS AAC) e conversao de formato | Media | M | Resolve a heuristica de duracao do fMP4 (ver review) e habilita FLAC/Opus sob demanda. ffmpeg deve ser opcional, detectado no PATH. |
| 8 | ~~Actions fixadas por SHA~~ | Concluido | — | Todas as actions por SHA; Dependabot mantem. |
| 9 | Assinatura Authenticode do instalador | Alta (distribuicao) | P tecnico / custo de certificado | Unica mitigacao definitiva para SmartScreen/Defender. Depende de o mantenedor adquirir certificado. |
| 10 | ~~Instalador com atalho e desinstalador~~ | Concluido v1.0 | — | NSIS do Tauri, por usuario, com bootstrapper do WebView2. |
| 11 | ~~i18n da interface~~ (CLI continua pt-BR) | Concluido v1.0 | — | 11 idiomas no app; traduzir a CLI fica para depois se houver demanda. |
| 12 | Persistencia opcional do historico de jobs | Baixa | P | Jobs vivem em memoria e somem ao fechar o app. |
| 13 | Validar shortlink `on.soundcloud.com` e watch longo ao vivo | Media | P | Implementados e testados sem rede; falta evidencia com o SoundCloud real. |
| 14 | Atualizacao automatica (`tauri-plugin-updater`) com assinatura | Media | M | Hoje o usuario baixa cada release manualmente. |

## Mapeamento do SoundCloud 2026 (endpoints extraidos do bundle web e testados sem login, 2026-09-30)

Evidencia: `charts?kind=top` (usado por scdl/yt-dlp) retorna 404; charts agora sao playlists de `music-charts-{regiao}`
listadas em `charts/selections`. Estacoes e trending sao `system-playlist` em `/discover/sets/<tipo>:<id>`.
`users/:id/likes` mistura faixas e playlists (flume: 144 faixas + 7 playlists; hoje so faixas sao baixadas).
Comentarios trazem `timestamp` em ms; `waveform_url` da 1800 amostras; 50% das faixas em trending tem ISRC;
26% sao previa Go+. Todo HLS novo inclui `abr_sq` (master playlist, ja suportado).

| # | Item | Prioridade | Esforco | Por que |
|---|---|---|---|---|
| 15 | Estacoes e trending: `/discover/sets/artist-stations:ID`, `track-stations:ID`, `trending-by-genre:GENERO` | Alta | P | Validador de URL rejeita `:`; o modelo ja aceita `system-playlist`. Nenhum concorrente baixa estacoes. |
| 16 | Playlists e albuns curtidos (`users/:id/likes` misto / `playlist_likes`) | Alta | P | Lacuna atual: `/likes` ignora playlists curtidas. Baixar como colecao, uma pasta por playlist. |
| 17 | Navegador de charts por regiao e genero (`charts/selections`) | Alta | M | Endpoint antigo de charts morreu; ninguem mais oferece. |
| 18 | Preservacao: detectar faixas da biblioteca removidas/privadas/Go+ no SoundCloud (`tracks?ids=` em lotes de 50) | Alta | M | Insight exclusivo: "voce tem a unica copia". Sem download, so metadados. |
| 19 | Deduplicacao por ISRC + duracao entre reuploads (biblioteca) | Media | P | Mesma gravacao enviada por selo e artista vira uma so copia local. |
| 20 | Radar de artistas: monitorar perfis (ou os `followings` de um usuario) e baixar uploads novos desde a ultima checagem | Media | M | Watch hoje e so de playlist; nenhum concorrente monitora artistas. |
| 21 | Completar o album a partir de uma faixa (`tracks/:id/albums`) e "onde esta faixa aparece" (`playlists_without_albums`) | Media | P | Colou um single, o app oferece o album inteiro. |
| 22 | Busca avancada: facetas de genero, `filter.created_at`, `filter.duration`, `filter.license=to_share` (Creative Commons), autocomplete `search/queries` | Media | P | CC = material liberado para remix/sample; nenhum downloader expoe. |
| 23 | Momentos: waveform + mapa de calor dos comentarios com tempo; exportar comentarios como `.lrc` sincronizado | Baixa | M | Players mostram os comentarios no momento certo, como letra. |
| 24 | Radio Perseus: estacao a partir de varios artistas-semente (`relatedartists` + estacoes), sem faixas ja na biblioteca | Baixa | M | Descoberta que so baixa novidade. |
| 25 | Ficha do artista: `web-profiles`, verificado, estacao, spotlight (faixas fixadas) e `artist.json` na pasta | Baixa | P | Contexto do artista junto dos arquivos. |
| 26 | Painel da biblioteca: horas, generos, artistas, bitrates, espaco, faixas com ISRC | Baixa | M | Visao do acervo; dados ja estao no archive/biblioteca. |

Descartados (exigem login ou assinatura): arquivo original (`/download` 401), Go+ `hq` 256 kbps, sets personalizados
(`weekly`, `new-for-you` 404 anonimo), estatisticas `/you/insights` e `stats/timeseries` (so o dono).

## Qualidade e robustez (achados da suite de testes, 2026-09-30)

| # | Item | Prioridade | Esforco | Por que |
|---|---|---|---|---|
| 27 | Lock entre processos no `.part` (CLI e app baixando a mesma pasta ao mesmo tempo) | Media | P | O lock atual e so dentro do processo; `fs4` ja e dependencia. |
| 28 | CI do core Rust tambem em Linux | Media | P | Hoje so Windows; paths e permissoes de arquivo divergem. |
| 29 | Build e E2E do app no macOS/Linux (WebKit real do Tauri) | Baixa | M | O E2E em WebKit roda so contra o front com IPC simulado. |
| 30 | Aumentar o score de mutacao de `describe.ts`/`translate.ts` (88%) | Baixa | P | Sobreviventes restantes sao strings e ramos de fallback. |
