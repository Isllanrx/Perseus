---
updated: 2026-09-30
escopo: v1.0 (Rust + Tauri), apos catalogo completo e suite de testes
---

# Revisao de arquitetura

## O que esta solido
- **Fatias verticais** (`inspect`, `download`, `watch`) sobre um kernel `shared`; CLI e app sao adaptadores finos
  sobre `run_download` e eventos tipados. Nenhuma regra de negocio nos adaptadores.
- **Fronteiras de confianca explicitas**: URL por allowlist com parse manual, bytes so de hosts permitidos
  (inclusive redirects), `ensure_within` em todo caminho, nomes saneados (Windows, C0/C1, bidi), archive
  validado na carga. Testado por propriedades e entradas hostis.
- **Idempotencia em camadas**: archive por pasta (id -> arquivo), fallback por nome, biblioteca global; nomes
  unicos por pasta; escrita atomica.
- **Resiliencia medida**: retry de transporte e de faixa separados e contados, retomada por `Range`,
  cancelamento imediato, isolamento de falha por lote e por tarefa (panico vira falha da faixa).
- **Contrato Rust <-> front verificado** por arquivos compartilhados; front sem acesso a rede (CSP `connect-src`
  so IPC) e com IPC minimo.

## Riscos e dividas
| Risco | Impacto | Mitigacao atual | Proximo passo |
|---|---|---|---|
| API nao oficial do SoundCloud (client_id do bundle, endpoints `api-v2`) | Alto | descoberta + renovacao unica, teste ao vivo e smoke | backlog: monitorar quebra no CI semanal com teste ao vivo |
| Lock de arquivo so dentro do processo | Medio | serializa jobs do app | backlog 27: lock no `.part` via `fs4` |
| Core testado so no Windows | Medio | E2E do front em 3 motores | backlog 28-29 |
| Estado do app em memoria (jobs somem ao fechar) | Baixo | archive/biblioteca persistem o essencial | backlog 12 |
| `library.json` cresce sem limite | Baixo | descarta entradas de arquivos sumidos ao salvar | medir com bibliotecas grandes antes de otimizar |
| Instalador sem assinatura | Medio (SmartScreen) | SHA256SUMS + proveniencia | certificado Authenticode |

## Decisoes relacionadas
[Fatias verticais](../decisions/arquitetura-fatias-verticais.md), [sem DRM](../decisions/sem-contorno-de-drm.md),
[Rust + Tauri](../decisions/reescrita-rust-tauri.md), [catalogo, biblioteca e sync](../decisions/catalogo-biblioteca-sync.md),
[estrategia de testes](../decisions/estrategia-de-testes.md).
