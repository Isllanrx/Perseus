# xtask

Project automation, run through Cargo: `cargo xtask <command>` (alias in `.cargo/config.toml`). Nothing here ships to
users, the installer or Vercel.

| Command | What it does |
| --- | --- |
| `comments` | Fails if any source file has a comment other than a single-line tool directive |
| `comments --strip [--report <file.md>]` | Removes comments with proof that the code did not change, and lists the removed text in a `.md` file (default: `target/comments-removed.md`) |
| `help` | Lists the commands |

## Comment policy

**Code has no comments.** Names, types and tests carry the meaning; explanations, decisions and processes live in
Markdown: `README.md`, `CONTRIBUTING.md`, `scripts/README.md`, `docs/context/*.md` and the decisions in
`docs/decisions/`. The report of each removal is produced by `comments --strip --report` and is not committed.

The only exception is a **single-line tool directive**, which changes how a tool behaves:

| Language | Directives kept |
| --- | --- |
| TypeScript/JavaScript | `// eslint-...`, `/* eslint-... */`, `// @ts-...`, `/// <reference .../>` and the `#!` shebang |
| YAML | `# zizmor: ...`, `# yaml-language-server: ...`, `# shellcheck ...` and the version after a SHA-pinned `uses:` (`@<sha> # v4.2.2`) |
| TOML | `#:schema ...` |
| PowerShell | `#Requires ...` |
| Rust, CSS, HTML | none |

A directive spanning more than one line is not a directive: it is a comment and gets removed.

### Rust doc comments that become program text

In Rust, `///` and `//!` are the `#[doc]` attribute, and some derives use that text at runtime: `clap` (`Parser`,
`Args`, `Subcommand`, `ValueEnum`) turns each doc comment into `--help` text. Removing the comment would change the
program. In those files the description goes in an explicit attribute (`#[arg(help = "...")]`,
`#[value(help = "...")]`, `#[command(about = "...")]`) and the file may not contain doc comments; if it does,
`comments` refuses the file and explains why instead of stripping it. The golden test
`crates/perseus-cli/tests/golden/help.txt` confirms that `--help` stays identical.

### Blocks that only held a comment

A `catch { /* ignore */ }` becomes `catch {}`, which ESLint (`no-empty`) would flag. An empty `catch` is allowed
(`no-empty` with `allowEmptyCatch` in `web/eslint.config.js`) because it means "best effort": local storage
unavailable, optional `.m3u8` or tags. Any other block left empty is still a lint error and needs explicit code.

## Why removal is safe (no regex)

Every tracked file (or new file not ignored by `.gitignore`) is read by a lexer for its language
(`src/lexers.rs`), which knows what is **not** a comment:

- **Rust:** strings, raw strings (`r#"..."#`), byte strings, chars, lifetimes and nested block comments.
- **TypeScript/JavaScript:** strings, template literals with nested `${...}`, regular expressions (based on the
  previous token) and division.
- **TSX/JSX:** JSX text and attributes are content (apostrophes and the `//` in URLs never become strings or
  comments); `{/* ... */}` is removed together with its braces; generics in arrow functions (`<K extends X>(...)`,
  `<A, B>(...)`) are not tags.
- **CSS and HTML:** strings, attributes, and the CSS/JS inside `<style>`/`<script>`.
- **TOML:** basic, literal and multi-line strings.
- **YAML:** quoted scalars, `|`/`>` blocks and the shell inside `run:` (bash or PowerShell, depending on `shell:` or
  the runner), including heredocs and here-strings.
- **PowerShell (`.ps1`):** strings, here-strings and `<# ... #>` blocks.

With `--strip`, a file is only written if the result **passes every check**:

1. the new file is read again by the same lexer without errors;
2. no comment remains other than directives;
3. the directives are exactly the same;
4. the code lines, with comments removed, are identical to the original.

If any check fails, the file is left untouched and the reason is printed. An unterminated string or block is an
error, never a guess. Document and data formats (`.md`, `.json`, ignore files, `.gitattributes`) are out of scope.

## Procedure

```powershell
cargo xtask comments                                   # the same check CI runs
cargo xtask comments --strip --report target/removed.md
cargo fmt --all                                        # removal can leave an enum or list that now fits on one line
```

Then move anything still useful from the report into the right `.md` file and run the project checks
(`pwsh scripts/test-all.ps1`). In CI, the `.github/workflows/comments.yml` workflow runs the xtask tests and the
check on every push and pull request.
