# Estrategia de testes por categoria

**Situacao:** Em vigor (2026-09-30)

## Contexto
O Perseus e um app desktop (Tauri + React) com core Rust que conversa com uma API de terceiros (SoundCloud) e
grava arquivos no disco do usuario. Nao ha servidor, banco SQL, login nem multiusuario. A lista padrao de
categorias de teste (funcional, UI, backend, banco, desempenho, seguranca, resiliencia, compatibilidade, dados,
tecnicas de codigo, operacao, cobertura) foi aplicada ao que existe de fato: cada categoria tem um teste real
ou um motivo documentado para nao se aplicar, com o equivalente que cobre o mesmo risco.

## Decisao
Orquestrador unico `scripts/test-all.ps1` (`-Sanity` rapido, padrao, `-Full` pesado, `-Offline`), log por etapa
em `target/test-reports/`. CI: `ci.yml` em cada push (inclui E2E em 3 motores e varredura de segredos) e
`quality.yml` semanal (fuzz, desempenho, mutacao).

### 1. Correcao funcional
| Categoria | Onde |
|---|---|
| Unidade | `#[test]` por modulo no core/Tauri; Vitest em `web/src/**` |
| Integracao | engine + client + wiremock; `JobManager` do Tauri; binario da CLI (`crates/perseus-cli/tests/cli.rs`) |
| Componente | Vitest + Testing Library (`CommandBar`, `Settings`, `ReleaseView`, `SearchResults`, `Sidebar`) |
| API | comandos IPC (testes do Tauri); cliente da API do SoundCloud contra wiremock (status, payload, paginacao, erros) |
| E2E | Playwright com backend IPC simulado; smoke real ponta a ponta |
| Regressao | testes nomeados pelos bugs: extensao do archive, colisao de template, traversal no archive, historico de URL (v0.1) |
| Smoke | `scripts/smoke.ps1` (binario de release contra o SoundCloud real) |
| Sanidade | `test-all.ps1 -Sanity` |
| Aceitacao | regras de `business-rules.md` como testes (sync nunca apaga, numeracao pela posicao, anonimo, DRM recusado) + smoke dos fluxos do usuario |
| Contrato | `contracts/log-events.json` (Rust gera, front exige traducao em 11 idiomas) e `contracts/job-in.json` (front envia, Rust aceita com `deny_unknown_fields`) |
| Snapshot/golden | `--help` da CLI (`tests/golden/help.txt`), `.m3u8` exato, eventos de log, baselines visuais |

### 2. Front / UI
| Categoria | Onde |
|---|---|
| UI e interacao | Vitest + `e2e/interaction.spec.ts` (clique, teclado, setas, Enter) |
| Regressao visual | `e2e/visual.spec.ts` (`toHaveScreenshot`, baseline no Chromium/Windows) |
| Cross-browser | Chromium, WebKit (motor do Tauri no Linux/macOS) e Firefox |
| Responsivo | 6 viewports de 768 a 1920 px, sem overflow horizontal |
| Acessibilidade | axe (WCAG 2.1 AA) + navegacao so por teclado |
| Formularios | limites dos ajustes, campo vazio desabilita acoes |
| Navegacao | historico de tarefas na barra lateral |
| i18n / l10n | 11 idiomas, plural CLDR, numeros/datas por `Intl`, arabe em RTL, contrato de eventos |
| Offline | fluxo completo sem nenhuma requisicao externa (privacidade); downloads exigem rede por natureza |

### 3. Backend / IPC
| Categoria | Onde / motivo |
|---|---|
| Endpoint | cada comando IPC e cada endpoint da API do SoundCloud usado |
| Autenticacao | N/A para usuario (produto anonimo). Equivalente: descoberta, cache e renovacao unica do `client_id` publico em 401 |
| Autorizacao | capabilities IPC minimas (so ouvir eventos) testadas; `open_folder` so abre pasta registrada pelo job |
| Validacao | `JobIn` com limites e campos desconhecidos recusados; URLs hostis; propriedades |
| Tratamento de erro | mapeamento de status para erros de dominio; codigos de saida da CLI |
| Concorrencia | dois jobs na mesma pasta; pico de requisicoes a API <= 8; limite de jobs |
| Idempotencia | re-sync nao baixa de novo; troca de template reaproveita pelo archive |
| Rate limit | 429 com `Retry-After`; limitador de banda; semaforo da API |
| Webhook | N/A: nao ha webhooks |

### 4. Banco de dados
Nao ha banco. A persistencia e `.perseus-archive.json` (por pasta) e `library.json` (global), e recebe os testes
equivalentes: CRUD (ida e volta), restricoes (so nomes simples de arquivo), transacao (`atomic_write` preserva o
conteudo anterior em falha), migracao/compatibilidade (arquivo sem versao e de versao futura), integridade
(entradas de arquivos sumidos descartadas), concorrencia (escritores simultaneos mesclados), backup/restauracao
(archive perdido reconstruido a partir dos arquivos). N/A: SQL, deadlock de banco, replicacao, desempenho de query.

### 5. Desempenho (`-Full`, release)
Carga (500 faixas com 503 injetados), pico (120 x 503), soak (60 rodadas + memoria), volume (5000 faixas em
100 lotes), escalabilidade (1/4/8/16 workers, exige >= 6x), capacidade (limites de workers e API), latencia e
vazao (faixas/s e MB/s no log), recursos (alocador contador para vazamento; checagem de espaco em disco).

Medicao de referencia (2026-09-30, release, Windows 11, servidor simulado local; valores de hardware, nao SLA):

| Teste | Resultado |
|---|---|
| Carga | 500 faixas em 5,3 s (8,3 MB/s), 40 x 503 absorvidos, re-sync sem nenhum download |
| Escalabilidade (150 ms de latencia por faixa) | 1 worker 4,9 faixas/s; 4 -> 20,4; 8 -> 39,7; 16 -> 70,8 (14,6x) |
| Pico | 120 x 503 absorvidos em 17,7 s (104 no retry de transporte, 16 no retry da faixa), 200/200 faixas |
| Soak | 60 rodadas: tempo estavel (61 ms -> 39 ms); memoria viva 2,2 MB -> 2,7 MB (limite do teste: +2 MB) |
| Volume | 5000 faixas hidratadas em 100 lotes em 0,37 s, pico de 8 requisicoes simultaneas a API |

Mutacao (2026-09-30): Rust 188 mutantes viaveis, 7 sobreviventes antes dos testes novos; apos eles, restam 2
equivalentes (`filesystem.rs` truncamento `>`/`>=` e `transcoding.rs` `rank` `-`/`/`, que preservam o resultado).
Front (Stryker, `lib` e `i18n`): 77,9% -> 93,4% apos testes direcionados (`api.ts` 100%, `format.ts` 97%, `locales.ts` 91%,
`describe.ts` 88%, `translate.ts` 88%).

### 6. Seguranca
| Categoria | Onde / motivo |
|---|---|
| Validacao de entrada | URLs hostis (DAST da CLI), propriedades, allowlist de hosts, `ensure_within` |
| Path traversal / spoofing | archive adulterado, templates, controles C0/C1 e bidi (U+202E) removidos de nomes |
| XSS | titulos com HTML renderizados como texto (Vitest e E2E) |
| Segredos | `scripts/scan-secrets.mjs` + teste de que o `client_id` nao aparece nos logs |
| Dependencias | cargo-deny (RustSec, licencas, origens), npm audit |
| SAST | clippy pedantic, ESLint estrito, CodeQL (Rust e TS) no CI |
| DAST | harness de entradas hostis contra o binario; nao ha superficie de rede (sem servidor) |
| Config | CSP sem `unsafe-*`, `connect-src` so IPC, instalador por usuario, versoes sincronizadas |
| N/A | SQL injection (sem SQL), CSRF e sessao (sem servidor nem cookies), container (sem imagem) |
| Pentest | fora do escopo automatizado; recomendado manualmente antes de distribuir em larga escala |

### 7. Resiliencia
Injecao de falha e caos (`RawServer` corta corpo no meio; 5xx aleatorios), queda de rede com retomada por
`Range`, `.part` corrompido descartado, timeout/cancelamento de transferencia travada, retry de transporte e de
faixa, falha de dependencia (lote da API isolado, erro fatal de resolucao), recuperacao (archive perdido).
Circuit breaker: o equivalente e o backoff progressivo do modo Monitorar (testado). N/A: failover e disaster
recovery de servidor (app local; o Perseus nunca apaga arquivos do usuario).

### 8. Compatibilidade
Motores Chromium/WebKit/Firefox; Windows (CI e local), Linux (CI do front/E2E). macOS e mobile: N/A (sem build).
Retrocompatibilidade: ajustes antigos no `localStorage`, archive sem versao. Compatibilidade futura: archive de
versao maior com campos extras. API do SoundCloud: teste ao vivo e smoke detectam quebra.

### 9. Dados
Validacao de audio (cabecalho, tamanho, duracao), integridade byte a byte apos retomada e com jobs concorrentes,
tags gravadas e relidas, qualidade (dedupe de faixas, hidratacao de stubs), reconciliacao (sync contra a
playlist; biblioteca contra o disco). N/A: ETL.

### 10. Tecnicas de codigo
Mutacao (cargo-mutants nos modulos de regra/seguranca; Stryker em `web/src/lib` e `i18n`), fuzz por geracao
(proptest, 20 mil casos; libFuzzer exige nightly), propriedades (18), analise estatica e tipos (clippy, tsc
strict), vazamento de memoria (soak com alocador contador), corrida (testes concorrentes; `unsafe` proibido fora
do alocador de teste). Loom: N/A (sem primitivas de sincronizacao proprias alem de `Mutex`).

### 11. Operacao
Health/readiness/liveness/alertas: N/A (app desktop, sem servico). Equivalentes: dialogo de falha na
inicializacao, instancia unica, teste de configuracao (CSP, capabilities, versoes), build do instalador com
checagem de PE no smoke, observabilidade (logs JSON com `run_id`, sem segredos). Rollback: releases anteriores
ficam no GitHub (sem auto-update).

### 12. Cobertura e qualidade
Linhas/regioes/funcoes no Rust (`cargo llvm-cov`, minimo 80% de linhas); linhas/funcoes/branches no front (v8).
Cobertura de branch no Rust exige nightly (N/A). Mutation score, taxa de aprovacao, flakiness (repeticoes em
`-Full`) e tempo por etapa no `summary.txt`.

## Consequencias
- A suite padrao continua rapida o bastante para cada push; o pesado fica semanal.
- Falhas de categoria N/A viram decisao explicita, nao esquecimento: se surgir servidor, banco ou login, esta
  estrategia precisa ser revista.
- Artefatos de build crescem com as suites; o orquestrador desliga o cache incremental, limpa a cobertura
  instrumentada e aborta abaixo de 15 GB livres.
