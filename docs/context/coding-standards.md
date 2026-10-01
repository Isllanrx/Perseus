# Padroes e armadilhas

## Comentarios
Decisao: [codigo sem comentarios](../decisions/codigo-sem-comentarios.md).
- Codigo sem comentarios; so diretiva de ferramenta de uma linha. Explicacao em `.md`. `cargo xtask comments` no CI
  (`.github/workflows/comments.yml`) e no `scripts/test-all.ps1`.
- Armadilha: `///` em struct/enum com derive do `clap` e o texto do `--help`. Use `#[arg(help = "...")]`,
  `#[value(help = "...")]`, `#[command(about = "...")]`; o xtask recusa doc comment nesses arquivos.
- Depois de `cargo xtask comments --strip`, rodar `cargo fmt --all` (enums/listas podem voltar a caber numa linha).
- `catch {}` vazio = melhor esforco intencional (ESLint `no-empty` com `allowEmptyCatch`); outro bloco vazio e erro.

## Rust
- Lints do workspace: clippy `pedantic` (CI com `-D warnings`), `unwrap_used` avisa; `unsafe` proibido no core e
  na CLI (`#![forbid(unsafe_code)]`). `expect` so com mensagem em invariantes (regex estaticas, testes).
- Erros esperados sao variantes de `perseus_core::Error`; `is_retryable()` decide retry e `kind()` alimenta a UI e
  os codigos de saida. `reqwest::Error` vira `Error::Transfer` sem a URL (pode conter client_id/assinatura).
- Futuros enviados a outra thread (comandos Tauri, `JoinSet::spawn`) precisam ser `Send`: nao usar closures em
  `stream::iter(..).map(..).buffered(..)` que emprestem variaveis do escopo — materializar os futuros num `Vec`
  antes (ver `fetch_tracks`). `#[tokio::test]` e single-thread e nao pega esse erro; so o build do app pega.
- Nada de I/O sincrono pesado no runtime: lofty (validacao/tags) e `read_dir` rodam em `spawn_blocking`.
- No Windows o `rename` do `.part` falha com o handle aberto: fechar (`drop`) o arquivo antes de validar/renomear.
- `directories::ProjectDirs` no Windows usa `%LOCALAPPDATA%\Perseus\{cache,data}`; logs ficam em `data\logs`.
- O `tracing-appender` com `max_log_files` falha se a pasta nao existir: criar antes (primeira execucao).
- lofty escreve ID3v2.3 com `WriteOptions::use_id3v23(true)` e avisa (via `log`) que troca UTF-8 por UTF-16:
  silenciado com `lofty=error` no filtro.
- `cargo build` puro do crate `perseus` gera um binario que carrega o `devUrl`; build de producao so via
  `npm run build` / `npx tauri build` (ativa `custom-protocol`).
- Construtores de teste (`SoundCloudClient::for_tests`, `HostPolicy::Loopback`) so existem em `cfg(test)` ou com a
  feature `test-util` (dev-dependency do app). Nunca habilitar em release.
- O `link.exe` em pt-BR imprime "Criando biblioteca..." em todo binario: lint `linker_messages` desligado.
- `Path::new(".mp3").extension()` e `None` (para o Rust e um arquivo oculto sem extensao). Comparar extensoes com
  `ext.eq_ignore_ascii_case(format.extension.trim_start_matches('.'))`. Esse erro deixou o archive inutil.
- `tokio::task_local!` nao atravessa `JoinSet::spawn`: cada tarefa precisa ser envolvida de novo no escopo
  (`count_transport_retries`).
- `#![forbid(unsafe_code)]` nao aceita `allow` local; o crate usa `forbid` fora de testes e `deny` em testes, onde so o
  alocador contador (vazamento de memoria) tem `allow`.
- `--all-features` + PDB completo no Windows fez o `target/` chegar a 34 GB: perfil `dev` usa
  `debug = "line-tables-only"` e dependencias sem simbolos. Para depurar variaveis: `CARGO_PROFILE_DEV_DEBUG=full`.
- Testes com `wiremock`: o primeiro mock registrado que casa vence; respostas "primeiro falha, depois ok" usam
  `up_to_n_times` montado antes do mock de sucesso. Lembrar que `run_download` resolve a URL de novo.
- Arquivo final tem tag ID3 no inicio: comparar audio com o original exige remover o cabecalho (`audio_payload`).

## Front-end
- Contrato IPC em `src-tauri/src/dto.rs` espelhado em `web/src/types.ts` (snake_case); mudar os dois juntos. Os
  testes de contrato (`contracts/`) quebram se um lado mudar sozinho.
- Stryker usa o runner `command` (o runner do Vitest nao executava testes no sandbox com espaco no caminho e
  reportava 14% falso). Testes que leem arquivos fora de `web/` nao entram no comando de mutacao.
- Vitest: `new URL("../x", import.meta.url)` vira import de asset pelo Vite e e bloqueado fora da raiz; ler
  arquivos compartilhados com `resolve(process.cwd(), "..", ...)`.
- Erros do IPC chegam como `{kind, code, message}` (serializacao do `CommandError`); `api.ts` converte em `ApiError`.
- `web/src/e2e/mockBackend.ts` so e importado em `vite --mode e2e`; a condicao `import.meta.env.MODE === "e2e"`
  e constante no build de producao e o modulo e eliminado (conferir com `grep mockIPC web/dist`).
- O `mockIPC` de eventos avisa "Couldn't find callback id" com o `StrictMode`; e artefato do mock, nao do app.
- TypeScript fixado em **6.0** (TS 7 nao expoe a API JS do typescript-eslint); ESLint fixado em **9**
  (jsx-a11y ainda sem ESLint 10). Nao usar `--legacy-peer-deps`.
- CSP proibe `data:` para fontes/scripts: Vite com `assetsInlineLimit: 0`.
- Nenhum texto de interface fixo em componente: chave em `i18n/messages/pt-BR.ts` + as 10 traducoes. Mensagens do
  backend exibidas ao usuario precisam de codigo estavel (nao traduzir comparando strings em pt-BR).
- Estado de erro/status guarda codigo + detalhe, nunca o texto pronto: o texto e montado no render para que a troca
  de idioma atualize a tela inteira.
- `getByText` do Playwright casa por substring: com o registro de atividade traduzido, usar `{ exact: true }`.
- CSS: usar propriedades logicas (`inline-end`, `text-align: start`) por causa do arabe (RTL); `letter-spacing`
  e zerado para ar/hi/bn/zh (quebra a ligacao das letras arabes).
- Mensagens do backend ficam em ASCII pt-BR (logs e fallback); a UI traduz pelos codigos.
- Working tree no Windows pode estar em CRLF (`.gitattributes` normaliza para LF no commit): scripts que
  processam texto devem tolerar `\r\n`.
