# Arquitetura

## Stack
Rust 1.90+ (edition 2024), workspace Cargo: tokio, reqwest 0.13 (rustls/aws-lc, HTTP/2), serde, lofty 0.25
(tags/validacao), m3u8-rs 6, tracing (+ subscriber JSON e appender rotativo), clap 4, directories. App desktop:
Tauri 2.12 (WebView2), plugins opener, dialog e single-instance. Front-end: React 19, TypeScript 6, Vite 8,
`@tauri-apps/api` 2. Instalador: NSIS do Tauri bundler. Decisao: [reescrita em Rust e Tauri](../decisions/reescrita-rust-tauri.md).

## Workspace
- `crates/perseus-core` — dominio, sem Tauri. Vertical Slice:
  - `features/inspect.rs`, `features/download/` (engine, audio, hls, planning, naming, validation, tagging,
    library [archive por pasta + biblioteca global], models), `features/watch.rs` (compoe download pela API publica `run_download`).
  - `shared/` — config, error, http, retry, filesystem, format, `soundcloud/` (auth, client, models, transcoding,
    urls).
  - `events.rs` — `Event` tipado + `Reporter` (log estruturado + sink do adaptador).
- `crates/perseus-cli` — binario `perseus-cli` (clap). Sem regra de negocio.
- `src-tauri` — binario `perseus` (janela). `commands.rs` (IPC), `jobs.rs` (JobManager), `dto.rs` (contrato).
- `crates/perseus-web` — backend HTTP da versao online (Axum): so metadados; `api/` — adaptador da funcao Vercel
  (`vercel_runtime`). Decisao: [versao web na Vercel](../decisions/versao-web-vercel.md).
- `xtask` — automacao (`cargo xtask comments`: politica de [codigo sem comentarios](../decisions/codigo-sem-comentarios.md)). So `std`; nao entra no
  app, no instalador nem na funcao da Vercel.
- `web/` — React; `dist/` e embutido pelo `tauri::generate_context!` em tempo de compilacao. `vite --mode web` gera a
  versao online (mesma UI, backend do navegador em `src/lib/web`).

## Fluxo de download
1. Adaptador canonicaliza a URL (`SoundCloudClient::canonicalize`: allowlist + expansao de shortlink hop a hop).
2. `run_download(client, request, cancel, reporter)`:
   - `resolve` -> `plan_download` (numeracao pela posicao na playlist completa, pasta, `ensure_within`).
   - Stubs hidratados via `fetch_tracks` (lotes de 50, ate 4 em paralelo, ordem preservada).
   - `build_job` por faixa (`choose_transcoding`; DRM/previa/regiao viram `unavailable`).
   - Faixas completas viram tarefas imediatamente; os lotes de stubs sao consultados em paralelo
     (`buffer_unordered(4)`) e cada lote agenda suas faixas ao chegar (paridade com o spider Scrapy da v0.2).
     Falha de lote marca so as faixas daquele lote; faixa ausente na resposta vira `not_returned`.
   - `Scheduler::claim_unique_name`: nomes de arquivo unicos por pasta (semeado pelo archive); template sem `{id}`
     que colida, ou nome truncado em 200 caracteres, ganha ` [id]`.
   - Cada job vira uma tarefa tokio num `JoinSet`, limitada por `Semaphore(workers)`; panico numa tarefa vira falha
     da faixa (mapa `task::Id -> track_id`); cancelamento por `CancellationToken` (futuro descartado; `.part`
     removido pelo guard `PartFile`).
   - `DownloadReport.stats` (`DownloadStats`): selecionadas, agendadas, tentativas, retries da faixa, retries de
     transporte (contador `task_local` por job em `http::count_transport_retries`, reaplicado em cada tarefa do
     `JoinSet`), copias da biblioteca, movidas para `Removed/`, bytes/s.
3. `AudioFetcher::download`, sob um lock por arquivo final (mapa global de `Weak<Mutex>`, chave minuscula): dois
   jobs do mesmo processo nunca escrevem o mesmo `.part`. Ordem: arquivo ja na pasta (pelo archive, depois pelo
   nome) -> copia da biblioteca global -> rede com retry (3x, backoff equal-jitter) de `stream_url` fresco ->
   `.part` -> validacao -> `rename` -> tags (best-effort) -> `TrackDownloaded`.
   - progressive: streaming em chunks com checagem de Content-Length e limite de 1 GiB; queda no meio mantem o
     `.part` e a proxima tentativa retoma com `Range` (so aceita `206` com `Content-Range` coerente).
   - HLS: `hls::segment_urls` (melhor variante, recusa DRM/byte-range, init map) -> segmentos em paralelo
     (`buffered(6)`, gravados em ordem), cada um com retry proprio.

## Mapa de chamadas HTTP (um `reqwest::Client` compartilhado)
| Chamador | Endpoint | Retry | Auth |
|---|---|---|---|
| `ClientIdProvider::discover` | `GET soundcloud.com` + bundles JS confiaveis | transporte (conexao/timeout/408/429/5xx, `Retry-After`) | — |
| `ClientIdProvider::verify` | `GET api-v2/search/tracks?limit=1` | transporte | candidato |
| `SoundCloudClient::expand_shortlink` | `GET on.soundcloud.com/*` sem redirect automatico, max 5 saltos | transporte | — |
| `resolve` / `fetch_tracks` / `stream_url` | `api-v2/resolve`, `api-v2/tracks?ids=`, `<transcoding.url>` | transporte + 1 refresh em 401/403 | client_id |
| `AudioFetcher` | CDN progressive, manifesto/segmentos HLS, artwork | transporte + retry da faixa (stream renovado) + retry por segmento | URL assinada |

Redirects do cliente principal so sao seguidos para hosts da allowlist (`redirect::Policy::custom`).
`SoundCloudClient` limita a 8 as requisicoes simultaneas a API (`Semaphore`, equivalente ao `CONCURRENT_REQUESTS`
do Scrapy); o permit cobre requisicao + corpo e e liberado antes de um refresh de client_id.

## Concorrencia
- `ClientIdProvider`: `tokio::sync::Mutex` (single-flight na descoberta/renovacao).
- Faixas: tarefas tokio (paralelismo real entre nucleos); CPU sincrona (lofty) em `spawn_blocking`.
- App: cada job e uma tarefa em `tauri::async_runtime`; limite de 3 ativos, 50 retidos, 5000 eventos por job.

## Observabilidade
- Todo `Event` e logado via `tracing` com `run_id` e `event` (nomes estaveis: `download_planned`,
  `track_downloaded`, `track_reused`, `track_copied`, `track_moved`, `playlist_file_written`, `download_retry`,
  `track_unavailable`, `track_failed`, `download_finished`, `download_cancelled`, `watch_*`); para a UI vai o
  `Event` serializado (tag `event` em snake_case), fixado em `contracts/log-events.json` e entregue ao sink do adaptador. No app, `run_id` = id do job.
- App: log JSON diario em `%LOCALAPPDATA%\Perseus\data\logs` (7 arquivos), filtro via `PERSEUS_LOG`.
- CLI: stderr texto/JSON, `--log-file` JSON, `--report-json` (`DownloadReport::to_json` com `summary`).

## App desktop (src-tauri)
- IPC: `get_config`, `inspect`, `search`, `list_jobs`, `create_job`, `cancel_job`, `job_events` (replay apos
  recarga), `open_folder` (so a pasta conhecida do job), `pick_output_dir(title, initial)` (dialogo nativo no Rust).
- Eventos: canal `perseus://job` com `{job_id, id, event: log|track|state|watch|end, data}`; `id` sequencial por
  job, gerado sob o mesmo lock da mudanca de estado. O front deduplica por id e nao aplica `state` antigo.
- `Job` recebe um `EventEmitter` (closure) em vez do `AppHandle`: testavel sem runtime do Tauri.
- Capabilities minimas (`core:event:allow-listen/unlisten`); plugins usados so do lado Rust.
- CSP em `tauri.conf.json` (imagens so de `*.sndcdn.com`, `connect-src ipc:`), `freezePrototype`.
- Single-instance: segunda execucao foca a janela existente. Fechar a janela cancela todos os jobs.
- `JobIn::validated` exige pasta de destino absoluta (o diretorio de trabalho do app e a pasta de instalacao).

## Versao online (Vercel)
- Rotas `GET /api/config|inspect|search` e `POST /api/plan|stream`; `vercel.json` reescreve `/api/<rota>` para a funcao
  unica `api/perseus.rs` com `?route=` (o router aceita tanto o caminho original quanto o reescrito).
- `plan` usa `perseus_core::features::download::plan_remote` (mesmas regras do `plan_download`, sem disco);
  `stream` so aceita URLs de transcoding (`/media/soundcloud:tracks:<id>/<uuid>/stream/(progressive|hls)`) e devolve
  a URL assinada ou as partes do HLS.
- Navegador (`web/src/lib/web`): `backend.ts` implementa os comandos/eventos do IPC; `engine.ts` baixa do CDN com
  `workers` (max 6), retry, validacao, tags (`tags/`), `.m3u8` e destino (`sink.ts`: pasta ou .zip).
  `pacer.ts`: um ritmo por aba para `plan`/`stream` (intervalo aleatorio 400-900 ms), abaixo dos 150/min por IP da
  funcao e do Firewall; cabe no plano Hobby (ver [versao web na Vercel](../decisions/versao-web-vercel.md)).
- `lib/backend.ts` escolhe Tauri ou navegador pelo modo do build; `lib/platform.ts` expõe `IS_WEB`.
- Dev: `cargo run -p perseus-vercel` (porta 3000) + `npm --prefix web run dev:web` (5173, proxy `/api`).

## Internacionalizacao (web/src/i18n)
- 11 locales (en, es, zh-CN, hi, fr, pt-BR, pt-PT, ar, bn, ru, id); pt-BR e o dicionario-fonte e define `MessageKey`;
  os demais sao `Record<MessageKey, Message>` (chave faltando quebra o `tsc`). Plurais por `Intl.PluralRules`.
- Runtime proprio, sem dependencia: `I18nProvider` (detecta `navigator.languages`, persiste em
  `perseus.locale.v1`, aplica `lang`/`dir` no `<html>`), `useI18n`, numeros/horas/unidades via `Intl`.
- Seletor na barra lateral mostra a sigla (EN, PT-BR...); o nome nativo fica em `aria-label`/`title`.
- O backend continua em pt-BR ASCII nos logs, mas manda dados estaveis para a UI traduzir: `CommandError.code`,
  `JobOut.error_code`, `reason_code` em faixas e eventos `track`, e `detail` (o `Event` serializado) nos eventos
  `log`/`watch`. `i18n/describe.ts` monta as frases; sem traducao, cai no texto original.
- RTL (arabe): propriedades logicas no CSS, marcador da tarefa ativa e lamina de progresso espelhados.

## Testes
Matriz completa em [estrategia de testes](../decisions/estrategia-de-testes.md).
- `crates/perseus-core/src/test_support.rs` (`cfg(test)`): helpers wiremock, `RawServer` (HTTP em TCP cru que corta
  o corpo no meio, honra `Range` e mede concorrencia) e o alocador contador `LIVE_BYTES` (`#[global_allocator]`).
- `features/download/resilience_tests.rs`: rede, concorrencia, persistencia e desempenho (`perf_*` ignorados).
- `src/property_tests.rs`: proptest; o mesmo conjunto vira fuzz com `PROPTEST_CASES=20000`.
- `contracts/` na raiz: `log-events.json` (gerado pelo Rust, lido pelo Vitest) e `job-in.json` (lido pelos dois).
- `crates/perseus-cli/tests/cli.rs` (binario real, golden em `tests/golden/`); `src-tauri/tests/config.rs`.
- `web/e2e/`: `perseus.spec.ts`, `interaction.spec.ts`, `visual.spec.ts` (baselines em `visual.spec.ts-snapshots/`).
- `web/e2e-web/` (`playwright.web.config.ts`): versao online em Chromium desktop, Pixel 7 e iPhone 14 (WebKit), com
  `/api` e CDN simulados; celulares de 320 a 414 px sem rolagem horizontal e alvos de toque >= 44 px.
- `scripts/`: `test-all.ps1` (orquestrador), `smoke.ps1` (real), `scan-secrets.mjs`, `collect-release.ps1`.
- CI: `ci.yml` (cada push; E2E em matriz chromium/webkit/firefox) e `quality.yml` (semanal: fuzz, desempenho, mutacao).
