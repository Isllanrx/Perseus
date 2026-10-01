use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::lexers::{Comment, Language, is_directive, lex, line_of};

pub struct Finding {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Default)]
struct Tally {
    findings: Vec<Finding>,
    failures: Vec<String>,
    directives: usize,
    changed: usize,
}

pub fn run(args: &[String]) {
    let strip = args.iter().any(|a| a == "--strip");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("o xtask fica na raiz do workspace")
        .to_path_buf();
    let report = args
        .iter()
        .position(|a| a == "--report")
        .and_then(|p| args.get(p + 1))
        .map_or_else(|| root.join("target/comments-removed.md"), PathBuf::from);

    let files = match tracked_files(&root) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("[ERRO] comments: {e}");
            std::process::exit(1);
        }
    };

    let mut tally = Tally::default();
    for (path, language) in &files {
        scan(&root, path, *language, strip, &mut tally);
    }

    println!(
        "  {} arquivos, {} comentarios, {} diretivas de ferramenta mantidas",
        files.len(),
        tally.findings.len(),
        tally.directives
    );
    let mut failed = !tally.failures.is_empty();
    if strip {
        if let Err(e) = write_report(&report, &tally.findings) {
            tally.failures.push(format!("{}: {e}", report.display()));
            failed = true;
        }
        println!(
            "  {} arquivos limpos; texto removido em {}",
            tally.changed,
            report.display()
        );
    } else if !tally.findings.is_empty() {
        eprintln!(
            "\n[ERRO] Comentarios no codigo ({}); explicacoes vao para arquivos .md:",
            tally.findings.len()
        );
        for f in &tally.findings {
            let first = f.text.lines().next().unwrap_or_default().trim();
            eprintln!("  {}:{}: {first}", f.path, f.line);
        }
        eprintln!("  `cargo xtask comments --strip` remove e lista o texto removido num .md.");
        failed = true;
    }
    if !tally.failures.is_empty() {
        eprintln!("\n[ERRO] comments:");
        for failure in &tally.failures {
            eprintln!("  {failure}");
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn scan(root: &Path, path: &str, language: Language, strip: bool, tally: &mut Tally) {
    let full = root.join(path);
    let Ok(src) = std::fs::read_to_string(&full) else {
        tally.failures.push(format!("{path}: nao e UTF-8"));
        return;
    };
    let comments = match lex(language, &src) {
        Ok(comments) => comments,
        Err(e) => {
            tally.failures.push(format!("{path}: {e}"));
            return;
        }
    };
    let (kept, removable): (Vec<Comment>, Vec<Comment>) = comments
        .into_iter()
        .partition(|c| is_directive(language, &src, *c));
    tally.directives += kept.len();
    if removable.is_empty() {
        return;
    }
    if language == Language::Rust && runtime_docs(&src, &removable) {
        tally.failures.push(format!(
            "{path}: mantido sem alteracao, doc comment aqui e texto de runtime (derive do clap); \
             converta para atributo explicito (help = \"...\")"
        ));
        return;
    }
    for c in &removable {
        tally.findings.push(Finding {
            path: path.to_string(),
            line: line_of(&src, c.start),
            text: src[c.start..c.end].to_string(),
        });
    }
    if !strip {
        return;
    }
    let stripped = strip_comments(&src, &removable);
    match verify(language, &src, &stripped) {
        Ok(()) => match std::fs::write(&full, stripped) {
            Ok(()) => tally.changed += 1,
            Err(e) => tally.failures.push(format!("{path}: {e}")),
        },
        Err(e) => tally
            .failures
            .push(format!("{path}: mantido sem alteracao, {e}")),
    }
}

fn tracked_files(root: &Path) -> Result<Vec<(String, Language)>, String> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let listing = String::from_utf8(output.stdout).map_err(|e| format!("git ls-files: {e}"))?;
    Ok(listing
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter_map(|p| Language::of(p).map(|l| (p.to_string(), l)))
        .filter(|(p, _)| root.join(p).is_file())
        .collect())
}

const DOC_READING_DERIVES: [&str; 4] = ["Parser", "Args", "Subcommand", "ValueEnum"];

fn is_doc_comment(text: &str) -> bool {
    (text.starts_with("///") && !text.starts_with("////"))
        || text.starts_with("//!")
        || (text.starts_with("/**") && !text.starts_with("/***") && text != "/**/")
        || text.starts_with("/*!")
}

fn reads_doc_comments(code: &str) -> bool {
    code.match_indices("derive(").any(|(at, needle)| {
        let list = &code[at + needle.len()..];
        let list = &list[..list.find(')').unwrap_or(list.len())];
        list.split(',').any(|name| {
            let name = name.trim();
            let name = name.rsplit("::").next().unwrap_or(name);
            DOC_READING_DERIVES.contains(&name)
        })
    })
}

pub fn runtime_docs(src: &str, removable: &[Comment]) -> bool {
    removable
        .iter()
        .any(|c| is_doc_comment(&src[c.start..c.end]))
        && reads_doc_comments(&code_lines(src, removable).join("\n"))
}

pub fn strip_comments(src: &str, removable: &[Comment]) -> String {
    let mut text = src.to_string();
    for c in removable.iter().rev() {
        let line_start = text[..c.start].rfind('\n').map_or(0, |p| p + 1);
        let line_end = text[c.end..].find('\n').map_or(text.len(), |p| c.end + p);
        let before = &text[line_start..c.start];
        let after = &text[c.end..line_end];
        if before.trim().is_empty() && after.trim().is_empty() {
            let (from, to) = if line_end < text.len() {
                (line_start, line_end + 1)
            } else {
                (line_start.saturating_sub(1), line_end)
            };
            text.replace_range(from..to, "");
            drop_doubled_blank_line(&mut text, from);
        } else {
            let (range, replacement) = inline_removal(&text, c.start, c.end, line_start, line_end);
            text.replace_range(range, replacement);
        }
    }
    text
}

fn inline_removal(
    text: &str,
    start: usize,
    end: usize,
    line_start: usize,
    line_end: usize,
) -> (std::ops::Range<usize>, &'static str) {
    let before = &text[line_start..start];
    let after = &text[end..line_end];
    if after.trim().is_empty() {
        let from = line_start + before.trim_end().len();
        let keep = if after.ends_with('\r') { "\r" } else { "" };
        return (from..line_end, keep);
    }
    if before.trim().is_empty() {
        let spaces = after.len() - after.trim_start_matches([' ', '\t']).len();
        return (start..end + spaces, "");
    }
    let left = text[..start].chars().next_back();
    let right = text[end..].chars().next();
    let joins =
        left.is_some_and(|l| !l.is_whitespace()) && right.is_some_and(|r| !r.is_whitespace());
    (start..end, if joins { " " } else { "" })
}

fn drop_doubled_blank_line(text: &mut String, at: usize) {
    let next_end = text[at..].find('\n').map(|p| at + p + 1);
    let Some(next_end) = next_end else {
        if text[at..].trim().is_empty() && text[..at].ends_with("\n\n") {
            text.truncate(at - 1);
        }
        return;
    };
    if !text[at..next_end].trim().is_empty() {
        return;
    }
    let previous_blank = at == 0 || {
        let previous_start = text[..at - 1].rfind('\n').map_or(0, |p| p + 1);
        text[previous_start..at].trim().is_empty()
    };
    let next_is_end = text[next_end..].trim().is_empty();
    if previous_blank || next_is_end {
        text.replace_range(at..next_end, "");
    }
}

pub fn verify(language: Language, original: &str, stripped: &str) -> Result<(), String> {
    let before = lex(language, original)?;
    let after = lex(language, stripped).map_err(|e| format!("o resultado nao e mais lido: {e}"))?;
    let left_over: Vec<usize> = after
        .iter()
        .filter(|c| !is_directive(language, stripped, **c))
        .map(|c| line_of(stripped, c.start))
        .collect();
    if !left_over.is_empty() {
        return Err(format!("sobraram comentarios nas linhas {left_over:?}"));
    }
    let directives = |src: &str, comments: &[Comment]| -> Vec<String> {
        comments
            .iter()
            .filter(|c| is_directive(language, src, **c))
            .map(|c| src[c.start..c.end].to_string())
            .collect()
    };
    if directives(original, &before) != directives(stripped, &after) {
        return Err("as diretivas de ferramenta mudaram".into());
    }
    let code_before = code_lines(original, &before);
    let code_after = code_lines(stripped, &after);
    if let Some(n) = (0..code_before.len().max(code_after.len()))
        .find(|&n| code_before.get(n) != code_after.get(n))
    {
        return Err(format!(
            "o codigo mudou: {:?} virou {:?}",
            code_before.get(n),
            code_after.get(n)
        ));
    }
    Ok(())
}

fn code_lines(src: &str, comments: &[Comment]) -> Vec<String> {
    let mut text = src.to_string();
    for c in comments.iter().rev() {
        let line_start = text[..c.start].rfind('\n').map_or(0, |p| p + 1);
        let line_end = text[c.end..].find('\n').map_or(text.len(), |p| c.end + p);
        let (range, replacement) = inline_removal(&text, c.start, c.end, line_start, line_end);
        text.replace_range(range, replacement);
    }
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn write_report(path: &Path, findings: &[Finding]) -> std::io::Result<()> {
    let mut out = String::from(
        "# Comentarios removidos do codigo\n\n\
         Gerado por `cargo xtask comments --strip`: o texto de cada comentario removido, por arquivo e linha\n\
         original. A politica e o processo estao em `xtask/README.md`.\n",
    );
    let mut current = "";
    for f in findings {
        if f.path != current {
            let _ = write!(out, "\n## {}\n\n", f.path);
            current = &f.path;
        }
        let lines: Vec<&str> = f.text.lines().map(str::trim).collect();
        let _ = writeln!(out, "- L{}: {}", f.line, lines.join(" "));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, out)
}

#[cfg(test)]
#[path = "comments_tests.rs"]
mod tests;
