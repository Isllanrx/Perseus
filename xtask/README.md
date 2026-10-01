# xtask

Automação do projeto, executada pelo Cargo: `cargo xtask <comando>` (alias em `.cargo/config.toml`). Nada daqui vai
para o usuário, para o instalador ou para a Vercel.

| Comando | O que faz |
| --- | --- |
| `comments` | Falha se algum arquivo de código tiver comentário que não seja diretiva de ferramenta de uma linha |
| `comments --strip [--report <arquivo.md>]` | Remove os comentários com prova de que o código não mudou e lista o texto removido num `.md` (padrão: `target/comments-removed.md`) |
| `help` | Lista os comandos |

## Política de comentários

**Código não tem comentários.** Nomes, tipos e testes carregam o significado; explicação, decisão e processo ficam
em `.md`: `README.md`, `CONTRIBUTING.md`, `scripts/README.md`, `docs/context/*.md` e as decisões em
`docs/decisions/`. O relatório de cada remoção é gerado por `comments --strip --report` e não é versionado.

A única exceção é a **diretiva de ferramenta de uma linha**, que muda o comportamento de uma ferramenta:

| Linguagem | Diretivas mantidas |
| --- | --- |
| TypeScript/JavaScript | `// eslint-...`, `/* eslint-... */`, `// @ts-...`, `/// <reference .../>` e o shebang `#!` |
| YAML | `# zizmor: ...`, `# yaml-language-server: ...`, `# shellcheck ...` e a versão depois de um `uses:` fixado por SHA (`@<sha> # v4.2.2`) |
| TOML | `#:schema ...` |
| PowerShell | `#Requires ...` |
| Rust, CSS, HTML | nenhuma |

Uma diretiva com mais de uma linha não é diretiva: é comentário e sai.

### Doc comments do Rust que viram texto do programa

Em Rust, `///` e `//!` são o atributo `#[doc]`, e alguns derives usam esse texto em tempo de execução: o `clap`
(`Parser`, `Args`, `Subcommand`, `ValueEnum`) transforma cada doc comment na descrição do `--help`. Remover o
comentário mudaria o programa. Nesses arquivos a descrição vai em atributo explícito
(`#[arg(help = "...")]`, `#[value(help = "...")]`, `#[command(about = "...")]`) e o arquivo não pode ter doc
comment; se tiver, o `comments` recusa o arquivo e diz por quê, em vez de removê-lo. O teste golden
`crates/perseus-cli/tests/golden/help.txt` confirma que o `--help` continua idêntico.

### Blocos que só tinham comentário

Um `catch { /* ignora */ }` vira `catch {}` e o ESLint (`no-empty`) passaria a acusar. O `catch` vazio é aceito
(`no-empty` com `allowEmptyCatch` em `web/eslint.config.js`) porque significa "melhor esforço": armazenamento local
indisponível, `.m3u8` ou tags opcionais. Qualquer outro bloco que fique vazio continua sendo erro de lint e precisa
de código explícito.

## Como a remoção é segura (sem regex)

Cada arquivo versionado (ou novo e não ignorado pelo `.gitignore`) é lido por um lexer da sua linguagem
(`src/lexers.rs`), que conhece o que **não** é comentário:

- **Rust:** strings, raw strings (`r#"..."#`), byte strings, caracteres, lifetimes e comentários de bloco aninhados.
- **TypeScript/JavaScript:** strings, template literals com `${...}` aninhados, expressões regulares (pelo contexto
  do token anterior) e divisão.
- **TSX/JSX:** texto e atributos JSX são conteúdo (apóstrofos e `//` de URLs não viram string nem comentário);
  `{/* ... */}` sai junto com as chaves; genéricos em arrow functions (`<K extends X>(...)`, `<A, B>(...)`) não são
  tags.
- **CSS e HTML:** strings, atributos, e o CSS/JS dentro de `<style>`/`<script>`.
- **TOML:** strings básicas, literais e multilinha.
- **YAML:** escalares com aspas, blocos `|`/`>` e o shell dentro de `run:` (bash ou PowerShell, conforme `shell:` ou
  o runner), com heredocs e here-strings.
- **PowerShell (`.ps1`):** strings, here-strings e blocos `<# ... #>`.

Com `--strip`, um arquivo só é gravado se o resultado **passar por todas as provas**:

1. o arquivo novo é lido de novo pelo mesmo lexer, sem erro;
2. não sobra nenhum comentário além das diretivas;
3. as diretivas continuam exatamente as mesmas;
4. as linhas de código, com os comentários retirados, são idênticas às do original.

Se qualquer prova falhar, o arquivo fica intacto e o motivo aparece na saída. Uma string ou bloco não terminado é
erro, nunca palpite. Formatos que são documento ou dado (`.md`, `.json`, arquivos de ignore, `.gitattributes`) ficam
fora.

## Procedimento

```powershell
cargo xtask comments                                   # checagem (a mesma do CI)
cargo xtask comments --strip --report target/removidos.md
cargo fmt --all                                        # a remocao pode deixar um enum ou lista que cabe numa linha
```

Depois, mova para o `.md` certo o que do relatório ainda for útil e rode as verificações do projeto
(`pwsh scripts/test-all.ps1`). No CI, o workflow `.github/workflows/comments.yml` roda os testes do xtask e a
checagem em cada push e pull request.
