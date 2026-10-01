# Reescrita do backend em Rust e app desktop Tauri 2

**Situacao:** Em vigor (2026-09-30). Substitui as decisoes da v0.2 ([historico](historico-v0.2-python.md)).

## Contexto
O executavel Python apresentava downloads lentos e com falhas. Causas identificadas na v0.2:
- Engine Scrapy em subprocesso `spawn`: no `.exe` do PyInstaller cada job relancava o binario inteiro e
  reimportava Scrapy/Twisted; o modo `auto` podia refazer tudo no engine direto apos um crash.
- Segmentos HLS baixados em serie numa thread; uma falha de segmento reiniciava a faixa inteira.
- AutoThrottle, GIL e `requests` sincrono limitavam o paralelismo real.
- Progresso da UI passava por log -> `QueueListener` -> SSE -> HTTP local, com token, CSRF e DNS rebinding a
  defender, e um zip de 52,5 MB com runtime Python embutido.

## Decisao
- **Core em Rust** (`crates/perseus-core`), async com tokio + reqwest (rustls, HTTP/2, um pool compartilhado),
  mantendo as fatias verticais `inspect`/`download`/`watch` e o kernel `shared`.
- **Um unico engine**: faixas como tarefas tokio (`JoinSet` + semaforo de `workers`), segmentos HLS em paralelo
  (`buffered`, gravados em ordem) com retry por segmento. Sem subprocesso, sem failover (nao ha mais engine que
  "caia" por infraestrutura).
- **Metadados e validacao com lofty** (substitui mutagen); **m3u8-rs** para HLS.
- **App desktop Tauri 2** (`src-tauri`): comandos IPC + eventos `perseus://job`; sem servidor HTTP, porta ou
  token. React mantido em `web/`, trocando `fetch`/`EventSource` por `invoke`/`listen`.
- **CLI** separada (`crates/perseus-cli`, clap) com as mesmas opcoes e codigos de saida (menos `--engine`/`--gui`).
- **Instalador NSIS** por usuario (Tauri bundler), com bootstrapper do WebView2.

## Alternativas consideradas
- *Otimizar o Python* (HLS paralelo, remover Scrapy): resolveria parte da lentidao, mas manteria o exe de 50 MB,
  o custo de inicializacao e a superficie HTTP local.
- *Electron*: bundle de ~100 MB e outro runtime JS; Tauri reaproveita o WebView2 do sistema.
- *Tauri 3*: ainda alpha em 2026-09; Tauri 2 e a linha estavel (Dependabot ignora major do Tauri).

## Consequencias
- Instalador de 5,2 MB; playlist de 11 faixas/50 MB em 3,4 s no app (medicao real).
- `perseus-core` testavel sem rede: `wiremock` + `HostPolicy::Loopback`, habilitado so em `cfg(test)` ou na
  feature `test-util` (dev-dependency do app; nunca em release).
- E2E Playwright passa a rodar a interface real contra um backend IPC simulado (`web/src/e2e/mockBackend.ts`,
  eliminado do build de producao); o backend real e coberto por `cargo test`.
- Teste de carga da API HTTP deixou de existir junto com a API HTTP.
- Requer Microsoft WebView2 (o instalador instala). O fallback "abrir no navegador" do pywebview foi substituido
  por um dialogo nativo de erro apontando a pasta de logs.
