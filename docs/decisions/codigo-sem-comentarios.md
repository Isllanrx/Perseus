# Codigo sem comentarios, verificado por lexers (`cargo xtask comments`)

**Situacao:** Em vigor (2026-09-30).

## Contexto
O codigo tinha 679 comentarios em 105 arquivos (Rust, TS/TSX, CSS, TOML, YAML e PowerShell), muitos repetindo o
que os `.md` de `docs/context/` e `docs/decisions/` ja registravam. Objetivo: codigo limpo, com no maximo
diretivas de uma linha, e todo o processo documentado so em `.md`, sem efeito colateral e sem remocao por regex.

## Decisao
- Codigo sem comentarios; excecao: diretiva de ferramenta de uma linha (eslint, `@ts-`, `/// <reference>`, versao de
  action fixada, `#:schema`, `#Requires`, zizmor, shellcheck). Explicacoes em `.md`.
- `xtask` (crate do workspace, so `std`): lexer por linguagem sem regex, remocao gravada so depois de provar que o
  resultado e lido de novo, sem comentario restante, com as mesmas diretivas e as mesmas linhas de codigo.
- Suporte alem do basico: modo JSX (texto JSX nao e string/comentario; `{/* */}` sai com as chaves;
  genericos `<K extends X>` em arrow nao sao tags), arquivos `.ps1` (ajuda `<# #>` sai, `#Requires` fica), shebang,
  `/* eslint- */` de uma linha, arquivos novos ainda nao versionados (`--others --exclude-standard`).
- **Doc comment de runtime:** em arquivo Rust com derive `Parser`/`Args`/`Subcommand`/`ValueEnum` o doc comment e o
  texto do `--help`; o xtask recusa o arquivo em vez de remover. A CLI passou a usar `help = "..."`.
- Verificacao no CI (`.github/workflows/comments.yml`: testes do xtask + checagem) e em `scripts/test-all.ps1`.

## Alternativas consideradas
- *Regex por linguagem*: quebra em strings, regex literais, template literals, JSX e heredocs; sem prova de que o
  codigo ficou igual. Descartado.
- *Permitir comentarios de uma linha quaisquer*: nao atende "codigo limpo" e reabre a duplicacao com os `.md`.
- *ESLint `no-warning-comments`/clippy*: cobrem so parte das linguagens e nao removem com prova.

## Consequencias
- A primeira limpeza removeu 679 comentarios; o que ainda era util foi movido
  para os `.md`, e os scripts ganharam `scripts/README.md` no lugar da ajuda `<# #>` (o `Get-Help` deles fica vazio).
- Encontrado na validacao: os doc comments da CLI eram o `--help`; o teste golden falhou, a CLI foi convertida e o
  xtask ganhou a protecao. Sem os testes isso teria passado.
- Tambem encontrado: 5 `catch` de melhor esforco ficaram vazios e o ESLint `no-empty` acusou; a regra passou a
  aceitar `catch` vazio (`allowEmptyCatch`), outros blocos vazios seguem proibidos.
- `cargo doc` deixa de ter descricoes; a documentacao do codigo e a dos `.md`.
- Depois de `--strip`, rodar `cargo fmt --all`.
