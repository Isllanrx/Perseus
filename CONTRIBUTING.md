# Contribuindo com o Perseus

Obrigado por querer ajudar. Este guia mostra como preparar o ambiente, validar uma mudança e enviá-la.

## Ambiente

Você precisa de [Rust 1.90+](https://rustup.rs/) (toolchain MSVC no Windows, com `rustfmt` e `clippy`) e de
[Node.js 24+](https://nodejs.org/).

```powershell
npm ci                  # CLI do Tauri
npm ci --prefix web     # interface
npm run dev             # aplicativo com hot reload
```

## Validação

Toda mudança precisa passar pelas mesmas etapas que a CI roda:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets     # pedantic; a CI trata avisos como erro
cargo test --workspace
cargo deny check                           # vulnerabilidades, licenças e origens

cd web
npm run check          # tipos, ESLint, Vitest com cobertura e build
npm run test:e2e       # Playwright: interface real com o backend IPC simulado
cd ..
```

Para rodar tudo de uma vez, com um log por etapa em `target\test-reports\` e resumo em `summary.txt`:

```powershell
pwsh scripts/test-all.ps1 -Sanity    # rapido, depois de uma mudanca: fmt, clippy, core, Vitest, E2E no Chromium
pwsh scripts/test-all.ps1            # padrao: tudo abaixo, menos as etapas pesadas
pwsh scripts/test-all.ps1 -Full      # + fuzz, desempenho, mutacao e deteccao de testes instaveis
pwsh scripts/test-all.ps1 -Offline   # sem as etapas que acessam o SoundCloud
```

O orquestrador desliga o cache incremental do Rust, limpa a cobertura instrumentada ao terminar e aborta se o
disco C: tiver menos de 15 GB livres. A matriz completa de categorias (incluindo as que não se aplicam e por quê)
está em [estratégia de testes](docs/decisions/estrategia-de-testes.md).

| Suíte | Onde | O que prova |
| --- | --- | --- |
| Unidade, integração e harness | `cargo test --workspace` | Regras de domínio e o engine contra um SoundCloud simulado (wiremock e servidor TCP caótico em `src/test_support.rs`) |
| Resiliência e persistência | `features/download/resilience_tests.rs` | Queda de rede no meio do arquivo com retomada por `Range`, `.part` corrompido, 429 com `Retry-After`, caos de 5xx, cancelamento, jobs concorrentes na mesma pasta, archive perdido/legado/futuro, escrita atômica |
| Propriedades e fuzz | `src/property_tests.rs` (`PROPTEST_CASES=20000` no modo fuzz) | URLs, nomes de arquivo (Windows, bidi, traversal), templates, parsers de JSON/archive/áudio nunca entram em pânico |
| Contrato Rust ↔ front | `contracts/` + `jobs.rs` + `web/src/contracts.test.ts` | Todo evento de log tem tradução nos 11 idiomas; o corpo que o front envia é aceito pelo Rust. Regenerar: `UPDATE_CONTRACTS=1 cargo test -p perseus -- contract` |
| CLI | `crates/perseus-cli/tests/cli.rs` | `--help` golden (`UPDATE_GOLDEN=1`), entradas hostis com código 2, logs JSON com `run_id` e sem `client_id` |
| Configuração e segurança | `src-tauri/tests/config.rs`, `scripts/scan-secrets.mjs` | CSP sem `unsafe-*`, IPC mínimo, instalador por usuário, versões sincronizadas, nenhum segredo versionado |
| Desempenho | `cargo test -p perseus-core --release -- --ignored --test-threads=1 load_ perf_` | Carga, escalabilidade 1→16 workers, pico de falhas, soak com medição de memória, volume de 5000 faixas |
| Cobertura Rust | `cargo llvm-cov --workspace --all-features --summary-only` | Mínimo de 80% de linhas |
| Mutação | `cargo mutants` (ver `test-all.ps1`) e `npm run test:mutation` | Os testes detectam bugs introduzidos de propósito |
| Ao vivo | `cargo test -p perseus-core -- --ignored --skip load_ --skip perf_` | Download HLS contra o SoundCloud real |
| Front | `npm run check` (em `web/`) | Tipos, ESLint, Vitest com cobertura e build |
| E2E | `npm run test:e2e` (em `web/`) | Chromium, WebKit e Firefox: fluxos, teclado, formulários, navegação, privacidade offline, XSS, axe, 6 viewports, RTL e regressão visual (`--update-snapshots` para aceitar mudança) |
| Smoke | `pwsh scripts/smoke.ps1` | CLI de release contra links reais: códigos de saída, curtidas, `.m3u8`, archive, idempotência, biblioteca, filtros, instalador |

Mudanças no fluxo de download também precisam ser vistas funcionando com um link real do SoundCloud (smoke ou o
aplicativo). Descreva no pull request o que você testou e anexe o log, se for relevante.

### Desempenho de referência

Testes de desempenho contra um servidor simulado local (release; medem o motor, não a conexão):

| Cenário | Resultado |
| --- | --- |
| 500 faixas com 40 erros 503 injetados | 5,3 s, nenhuma falha; a segunda rodada não baixa nada |
| Escalabilidade (150 ms de latência por faixa) | 1 download simultâneo: 4,9 faixas/s; 16 simultâneos: 70,8 faixas/s (14,6x) |
| Rajada de 120 erros 503 com 16 downloads | 200 de 200 faixas concluídas |
| 60 rodadas seguidas | tempo estável e memória viva estável (2,2 MB → 2,7 MB) |
| Playlist de 5000 faixas | metadados completados em 100 lotes em 0,37 s |

## CI/CD

| Workflow | Quando roda | O que faz |
| --- | --- | --- |
| [`ci.yml`](.github/workflows/ci.yml) | Push em `main` e `dev` e pull requests | Tipos, ESLint, Vitest com cobertura e build da interface; rustfmt, clippy pedantic e testes Rust no Windows; E2E com Playwright em Chromium, WebKit e Firefox; varredura de segredos, cargo-deny e npm audit; lint dos workflows; build do instalador |
| [`comments.yml`](.github/workflows/comments.yml) | Push em `main` e `dev` e pull requests | Testes do `xtask` e checagem de código sem comentários |
| [`quality.yml`](.github/workflows/quality.yml) | Semanalmente e sob demanda | Fuzz com 20 mil casos, testes de desempenho e testes de mutação (cargo-mutants e Stryker) |
| [`security.yml`](.github/workflows/security.yml) | Pull requests, push em `main` e `dev` e semanalmente | CodeQL para Rust e TypeScript, varredura de segredos em todo o histórico com gitleaks e revisão de dependências |
| [`scorecard.yml`](.github/workflows/scorecard.yml) | `main` e semanalmente | OpenSSF Scorecard |
| [`release.yml`](.github/workflows/release.yml) | Push em `main` com versão nova, tag `v*` ou sob demanda | Compila instalador e CLI do zero, gera `SHA256SUMS`, atesta a proveniência e publica uma **pre-release** |
| [`dependabot.yml`](.github/dependabot.yml) | Semanalmente | Propõe atualizações de actions, crates e pacotes npm |

Todo job começa com permissões somente de leitura, e só o job de publicação recebe permissão de escrita.

### Publicando uma release

1. Atualize a versão em `Cargo.toml` (workspace), `src-tauri/tauri.conf.json`, `package.json` e
   `web/package.json` num pull request.
2. O fluxo é `dev` → pull request → `main`. No merge, o `release.yml` lê a versão do `Cargo.toml` e, se a tag
   `v<versão>` ainda não existir, compila, cria a tag no commit do merge e publica a pre-release. Um merge sem
   mudança de versão não gera release. Uma tag `v*` enviada manualmente também publica.
3. Teste o instalador com links reais e, então, marque a release como definitiva no GitHub.

`pwsh scripts/collect-release.ps1` reúne localmente o instalador, a CLI zipada e o `SHA256SUMS` em `dist\`.

## Padrões

- Siga a arquitetura em fatias verticais: um caso de uso novo vai em `crates/perseus-core/src/features/`; só
  infraestrutura transversal vai em `crates/perseus-core/src/shared/`. CLI e app (`src-tauri`) não têm regra de
  negócio.
- Sem `unwrap()` em código de produção e sem `unsafe` no core e na CLI. TypeScript `strict`, sem `any`.
- Erros esperados são variantes de `perseus_core::Error`; nada é suprimido em silêncio.
- Sem `println!` fora da CLI. Progresso que interessa à interface é um `Event` emitido pelo `Reporter` (vira log
  estruturado e evento IPC ao mesmo tempo).
- O contrato IPC vive em `src-tauri/src/dto.rs` e é espelhado em `web/src/types.ts`; mude os dois juntos.
- Textos da interface nunca são escritos direto no componente: toda chave nasce em
  `web/src/i18n/messages/pt-BR.ts` e precisa existir nos outros 10 idiomas (o `tsc` falha se faltar uma, e o
  teste de dicionários recusa placeholders desconhecidos). Mensagens do backend exibidas ao usuário levam um
  código estável (`code`, `reason_code` ou o evento tipado em `detail`) para serem traduzidas.
- **Sem comentários no código** (Rust, TypeScript/TSX, JavaScript, CSS, HTML, TOML, YAML e PowerShell). Nomes, tipos e
  testes carregam o significado; explicações, decisões e processos vão em `.md` (`README.md`, `scripts/README.md`,
  `docs/context/`, decisões em `docs/decisions/`). A única exceção é a diretiva de ferramenta de **uma linha**
  (`// eslint-...`, `// @ts-...`, a versão depois de um `uses:` fixado por SHA, `#Requires`...). `cargo xtask comments`
  falha no CI se houver comentário; `cargo xtask comments --strip` remove com prova de que o código não mudou.
  Em structs do `clap`, a descrição vai em `#[arg(help = "...")]`, nunca em doc comment. Detalhes em
  [`xtask/README.md`](xtask/README.md).
- Nunca contorne DRM nem outras proteções. Pull requests nesse sentido não serão aceitos.

## Commits e pull requests

- Mensagens de commit explicam **por que** a mudança existe, não só o que mudou.
- Não use `--no-verify` nem force push em `main`.
- Um pull request por assunto, com testes cobrindo o comportamento novo ou o bug corrigido.

## Segurança

Falhas de segurança não vão em issues públicas. Veja o [SECURITY.md](SECURITY.md).
