---
updated: 2026-09-30
sprint: v1.0 — reescrita em Rust + app Tauri
---

# Sprint atual: v1.0 — reescrita em Rust + app Tauri

| Item | Status | Notas |
|---|---|---|
| `perseus-core`: URLs, client_id, API, modelos, transcoding (paridade com v0.2) | Concluido | testes portados dos casos pytest |
| Engine unico assincrono (tokio), HLS com segmentos em paralelo e retry por segmento | Concluido | validado ao vivo (progressive e HLS AAC) |
| Tags e validacao com lofty (ID3v2.3, MP4, Opus + capa) | Concluido | |
| Monitoramento (watch) com retentativa limitada por faixa | Concluido | logica de settle testada; execucao longa ao vivo pendente |
| CLI `perseus-cli` com as mesmas opcoes e codigos de saida | Concluido | sem `--engine`/`--gui` |
| App Tauri 2: IPC, eventos, replay, single-instance, dialogo de pasta, erro de inicializacao | Concluido | validado dirigindo o app real via CDP |
| React migrado para `invoke`/`listen`, progresso % por faixa | Concluido | cobertura 97,5% linhas |
| E2E Playwright com backend IPC simulado | Concluido | fluxos, a11y, 6 viewports, idiomas |
| Instalador NSIS (5,2 MB) + CLI zipada + SHA256SUMS | Concluido | `scripts/collect-release.ps1` |
| CI/CD reescrito (Rust no Windows, cargo-deny, CodeQL Rust) | Concluido | actionlint local OK; aguardando primeira execucao no GitHub |
| Remocao do Python (codigo, testes, PyInstaller, uv) | Concluido | decisoes da v0.2 mantidas em `docs/decisions/historico-v0.2-python.md` |
| Paridade com o crawler Scrapy: hidratacao em streaming, falha isolada por lote, panico -> falha da faixa, limite de 8 req. a API, `stats` | Concluido | auditoria 2026-09-30 |
| Pasta de destino absoluta obrigatoria no app; velocidade media e retries no resumo da CLI | Concluido | |
| Interface em 11 idiomas com seletor por sigla, deteccao do sistema e RTL | Concluido | Vitest 70 testes, E2E 13 (inclui arabe + axe) |
| Catalogo completo sem login: abas do perfil, colecoes por pasta, relacionadas, busca | Concluido | ao vivo: likes 77 faixas, albums, sets, popular, related, busca |
| Qualidade Melhor, template de nome, filtros de duracao, limite de banda, capa original | Concluido | |
| Archive por pasta, biblioteca global (copia sem rede), sync para `Removed/`, `.m3u8`, resume Range, checagem de espaco | Concluido | Rust 88 testes, Vitest 77 (93,6% linhas), E2E 14 |
| Suite de testes por categoria + CI em 3 navegadores + qualidade semanal | Concluido | 15/15 etapas; Rust 141, front 113, E2E 70; mutacao Rust so equivalentes, front 93,4% |
| Protecao de disco (perfil de debug leve e checagem de espaco no orquestrador) | Concluido | target/debug 26,8 GB -> 1,5 GB |
| Versao online na Vercel: `perseus-web` + `api/`, `plan_remote` no core, backend do navegador, tags ID3/MP4 em TS, ZIP/pasta, UI responsiva (Android/iOS) | Concluido | Rust core 129 + web 13; Vitest 163; E2E web 27 (3 perfis); build Linux validado em amazonlinux:2023; download real sob a CSP de producao |
| Ritmo randomizado no navegador + limites alinhados ao plano Hobby (150/min, `maxDuration` 30 s, gzip, cache CDN) | Concluido | regra do Firewall documentada no README |
| Validacao do app desktop apos a versao web | Concluido | `test-all.ps1` (E2E desktop 3 navegadores + visual, ao vivo, smoke) e instalador NSIS regerado |
| Deploy na Vercel + dominio `isllan.dev` | Pendente | passos no README (secao Versao online) |
| Codigo sem comentarios: `xtask` com lexers, 679 removidos com prova, workflow `comments.yml` | Concluido | golden do `--help` pegou doc comments do clap (convertidos para `help =`); `catch` vazio liberado no ESLint |
| Limpeza: docs em `docs/`, dependencias sem uso removidas, `.vercelignore` por lista de permissao, `yoke-derive` 0.8.4 | Concluido | `cargo deny` volta a passar; Rust 174 testes, Vitest 163 |
| Shortlink `on.soundcloud.com` validado ao vivo | Pendente | |
| Assinatura Authenticode do instalador | Pendente | depende de certificado |

## Riscos da sprint
- Primeira execucao da CI nova no GitHub ainda nao observada (CodeQL Rust, cargo-deny, build NSIS no runner).
- `devUrl`: build do crate `perseus` fora do CLI do Tauri gera binario de desenvolvimento (ver coding-standards).
