# Scripts

Automação local e de release. Os scripts não têm comentários (política em [`xtask/README.md`](../xtask/README.md));
o que cada um faz está aqui.

| Script | Uso | O que faz |
| --- | --- | --- |
| `test-all.ps1` | `pwsh scripts/test-all.ps1 [-Sanity] [-Full] [-Offline]` | Roda as verificações do Perseus e grava um log por etapa em `target\test-reports` (resumo em `summary.txt`). Não para na primeira falha; sai com 1 se alguma etapa falhar. |
| `smoke.ps1` | `pwsh scripts/smoke.ps1` | Smoke test contra o SoundCloud real: binários de release, resolução, download, idempotência e códigos de saída. |
| `collect-release.ps1` | `pwsh scripts/collect-release.ps1 -OutDir dist` | Reúne os artefatos de release (instalador NSIS e CLI zipada) e gera `SHA256SUMS`. |
| `scan-secrets.mjs` | `node scripts/scan-secrets.mjs` | Varre os arquivos versionados atrás de segredos: chaves privadas, tokens de provedores e client_id do SoundCloud em claro. Sai com 1 se encontrar algo. |

## `test-all.ps1`

- **Padrão:** segredos, comentários (`cargo xtask comments`), formato, clippy, testes Rust (unidade, integração,
  harness, contrato, regressão, resiliência, concorrência, persistência e propriedades), cobertura Rust, cargo-deny,
  testes da CLI, tipos/lint/Vitest com cobertura/build do front, npm audit, E2E em Chromium, WebKit e Firefox
  (acessibilidade, teclado, i18n/RTL, responsividade, privacidade offline e regressão visual), ao vivo e smoke.
- **`-Sanity`:** verificação rápida após uma mudança: formato, clippy, testes Rust do core, Vitest e E2E só no Chromium.
- **`-Full`:** acrescenta etapas pesadas: fuzz (20 mil casos por propriedade), desempenho (carga, escalabilidade,
  pico, soak, volume, memória), mutação (cargo-mutants e Stryker) e detecção de testes instáveis (repetições).
- **`-Offline`:** pula as etapas que acessam o SoundCloud real (ao vivo e smoke).
- Usa `CARGO_INCREMENTAL=0`: rodadas de teste não se beneficiam do cache incremental, que chegou a 13 GB no `target/`.
- Aborta antes de uma etapa se o C: tiver menos de 15 GB livres.

## `smoke.ps1`

Requer rede. Usa `target\release\perseus-cli.exe` (`cargo build --release -p perseus-cli`) e, se existir, confere o
instalador NSIS. Termina com código 1 na primeira verificação que falhar.

## `collect-release.ps1`

Rode depois de `npm run build` (instalador) e `cargo build --release -p perseus-cli`. O `SHA256SUMS` usa o formato do
`sha256sum` (`<hash>  <arquivo>`) para `sha256sum --check` no CI.

## `scan-secrets.mjs`

Os identificadores fictícios usados pelos testes ficam numa lista de permissões dentro do script.
