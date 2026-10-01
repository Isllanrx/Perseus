use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization as _;

use crate::shared::error::{Error, Result};

static INVALID_CHARS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[<>:"/\\|?*\p{Cc}\x{200E}\x{200F}\x{202A}-\x{202E}\x{2066}-\x{2069}]"#)
        .expect("regex valida")
});
static COLLAPSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s_]+").expect("regex valida"));

const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn trim_edges(text: &str) -> &str {
    text.trim_matches(|c| c == ' ' || c == '.')
}

pub fn sanitize_filename(name: Option<&str>, max_length: usize, fallback: &str) -> String {
    let Some(name) = name.filter(|value| !value.is_empty()) else {
        return fallback.to_owned();
    };
    let normalized: String = name.nfc().collect();
    let replaced = INVALID_CHARS.replace_all(&normalized, "_");
    let collapsed = COLLAPSE.replace_all(&replaced, " ");
    let mut cleaned = trim_edges(&collapsed).to_owned();
    if cleaned.chars().count() > max_length {
        let truncated: String = cleaned.chars().take(max_length).collect();
        truncated
            .trim_end_matches([' ', '.'])
            .clone_into(&mut cleaned);
    }
    if cleaned.is_empty() {
        return fallback.to_owned();
    }
    let stem = cleaned.split('.').next().unwrap_or_default().to_uppercase();
    if WINDOWS_RESERVED.contains(&stem.as_str()) {
        cleaned.insert(0, '_');
    }
    cleaned
}

pub fn sanitize(name: Option<&str>, fallback: &str) -> String {
    sanitize_filename(name, 120, fallback)
}

fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other),
        }
    }
    Some(out)
}

pub fn ensure_within(base: &Path, target: &Path) -> Result<PathBuf> {
    let escape = || {
        Error::InvalidInput(format!(
            "Caminho fora do diretorio permitido: {}",
            target.display()
        ))
    };
    let base_abs = std::path::absolute(base)?;
    let target_abs = std::path::absolute(target)?;
    let (Some(base_norm), Some(target_norm)) =
        (lexical_normalize(&base_abs), lexical_normalize(&target_abs))
    else {
        return Err(escape());
    };
    if !target_norm.starts_with(&base_norm) {
        return Err(escape());
    }
    if let (Ok(real_base), Ok(real_target)) = (base_norm.canonicalize(), target_norm.canonicalize())
        && !real_target.starts_with(&real_base)
    {
        return Err(escape());
    }
    Ok(target_norm)
}

pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::Builder::new()
        .prefix(&format!(
            ".{}.",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("tmp")
        ))
        .suffix(".tmp")
        .tempfile_in(parent)?;
    temp.write_all(content)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|err| Error::Io(err.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_cases() {
        let cases: [(Option<&str>, &str); 8] = [
            (Some(r#"a<b>c:"d/e\f|g?h*i"#), "a b c d e f g h i"),
            (Some(".."), "untitled"),
            (Some("../../etc/passwd"), "etc passwd"),
            (Some("CON"), "_CON"),
            (Some("nul.txt"), "_nul.txt"),
            (Some("  trailing dots...  "), "trailing dots"),
            (Some("tab\tand\nnewline"), "tab and newline"),
            (None, "untitled"),
        ];
        for (input, expected) in cases {
            assert_eq!(sanitize(input, "untitled"), expected, "input: {input:?}");
        }
    }

    #[test]
    fn sanitize_filename_truncates_by_chars() {
        assert_eq!(
            sanitize_filename(Some(&"x".repeat(500)), 50, "u")
                .chars()
                .count(),
            50
        );
        assert_eq!(sanitize_filename(Some(&"é".repeat(10)), 4, "u"), "éééé");
    }

    #[test]
    fn ensure_within_blocks_escape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let inside = dir.path().join("a").join("b");
        assert!(ensure_within(dir.path(), &inside).is_ok());
        assert!(ensure_within(dir.path(), &dir.path().join("..").join("escape")).is_err());
        assert!(
            ensure_within(
                dir.path(),
                &dir.path().join("a").join("..").join("..").join("x")
            )
            .is_err()
        );
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("nested").join("file.json");
        atomic_write(&target, b"{}").expect("primeira escrita");
        atomic_write(&target, br#"{"a": 1}"#).expect("segunda escrita");
        assert_eq!(
            std::fs::read_to_string(&target).expect("leitura"),
            r#"{"a": 1}"#
        );
        let entries: Vec<_> = std::fs::read_dir(target.parent().expect("pai"))
            .expect("dir")
            .collect();
        assert_eq!(entries.len(), 1);
    }
}
