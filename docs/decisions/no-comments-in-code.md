# No comments in code, enforced by lexers (`cargo xtask comments`)

**Status:** Active (2026-09-30).

## Context
The code had 679 comments across 105 files (Rust, TS/TSX, CSS, TOML, YAML and PowerShell), many of them repeating
what the Markdown in `docs/context/` and `docs/decisions/` already recorded. The goal: clean code, with at most
single-line directives, and the whole process documented only in Markdown, with no side effects and no regex-based
removal.

## Decision
- No comments in code; the exception is a single-line tool directive (eslint, `@ts-`, `/// <reference>`, the
  version of a pinned action, `#:schema`, `#Requires`, zizmor, shellcheck). Explanations go in Markdown.
- `xtask` (a workspace crate, `std` only): a regex-free lexer per language, with removal written only after proving
  that the result parses again, has no remaining comments, keeps the same directives and has the same code lines.
- Beyond the basics: a JSX mode (JSX text is neither a string nor a comment; `{/* */}` goes away with its braces;
  `<K extends X>` generics in arrow functions are not tags), `.ps1` files (`<# #>` help is removed, `#Requires`
  stays), shebangs, single-line `/* eslint- */`, and new files not yet tracked (`--others --exclude-standard`).
- **Runtime doc comments:** in a Rust file deriving `Parser`/`Args`/`Subcommand`/`ValueEnum`, the doc comment is the
  `--help` text; the xtask refuses the file instead of stripping it. The CLI moved to `help = "..."`.
- Enforced in CI (`.github/workflows/comments.yml`: xtask tests plus the check) and in `scripts/test-all.ps1`.

## Alternatives considered
- *Regex per language*: breaks on strings, regex literals, template literals, JSX and heredocs, with no proof that
  the code stayed the same. Rejected.
- *Allow any single-line comment*: does not deliver "clean code" and reopens the duplication with the Markdown.
- *ESLint `no-warning-comments` / clippy*: only cover some of the languages and cannot remove comments with proof.

## Consequences
- The first cleanup removed 679 comments; whatever was still useful moved into Markdown, and the scripts gained
  `scripts/README.md` in place of their `<# #>` help (their `Get-Help` is now empty).
- Found during validation: the CLI's doc comments were its `--help`; the golden test failed, the CLI was converted
  and the xtask gained the safeguard. Without the tests this would have slipped through.
- Also found: 5 best-effort `catch` blocks became empty and ESLint `no-empty` flagged them; the rule now allows
  empty `catch` blocks (`allowEmptyCatch`), while other empty blocks are still forbidden.
- `cargo doc` no longer has descriptions; the Markdown is the code's documentation.
- After `--strip`, run `cargo fmt --all`.
