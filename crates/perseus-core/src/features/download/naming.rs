use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::shared::error::{Error, Result};
use crate::shared::filesystem::{sanitize, sanitize_filename};

pub const SINGLES_FOLDER: &str = "Single Tracks";
static NUMBER_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\. ").expect("regex valida"));
static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{([a-z_]*)\}").expect("regex valida"));

pub const TEMPLATE_FIELDS: [&str; 8] = [
    "number", "artist", "title", "album", "year", "id", "genre", "uploader",
];
const MAX_TEMPLATE_LENGTH: usize = 200;

pub fn validate_template(template: &str) -> Result<()> {
    let invalid = |message: String| Error::InvalidInput(message);
    if template.trim().is_empty() || template.chars().count() > MAX_TEMPLATE_LENGTH {
        return Err(invalid(format!(
            "o template deve ter entre 1 e {MAX_TEMPLATE_LENGTH} caracteres"
        )));
    }
    let fields: Vec<&str> = PLACEHOLDER
        .captures_iter(template)
        .map(|c| c.get(1).map_or("", |m| m.as_str()))
        .collect();
    if let Some(unknown) = fields.iter().find(|field| !TEMPLATE_FIELDS.contains(field)) {
        return Err(invalid(format!(
            "campo desconhecido no template: {{{unknown}}}"
        )));
    }
    if !fields.iter().any(|field| matches!(*field, "title" | "id")) {
        return Err(invalid(
            "o template precisa de {title} ou {id} para distinguir as faixas".into(),
        ));
    }
    Ok(())
}

pub struct TemplateValues<'a> {
    pub number: &'a str,
    pub artist: &'a str,
    pub title: &'a str,
    pub album: &'a str,
    pub year: Option<u16>,
    pub id: i64,
    pub genre: Option<&'a str>,
    pub uploader: &'a str,
}

pub fn render_template(template: &str, values: &TemplateValues<'_>) -> String {
    let rendered = PLACEHOLDER.replace_all(template, |caps: &regex::Captures<'_>| {
        let raw = match caps.get(1).map_or("", |m| m.as_str()) {
            "number" => values.number.to_owned(),
            "artist" => values.artist.to_owned(),
            "title" => values.title.to_owned(),
            "album" => values.album.to_owned(),
            "year" => values.year.map(|y| y.to_string()).unwrap_or_default(),
            "id" => values.id.to_string(),
            "genre" => values.genre.unwrap_or_default().to_owned(),
            "uploader" => values.uploader.to_owned(),
            _ => String::new(),
        };
        sanitize(Some(&raw), "")
    });
    sanitize_filename(Some(&rendered), MAX_TEMPLATE_LENGTH, "Untitled")
}

#[derive(Debug, Clone)]
pub enum FileNaming {
    Numbered {
        number: usize,
        total: usize,
        display: String,
    },
    Template {
        stem: String,
    },
}

impl FileNaming {
    pub fn file_name(&self, extension: &str) -> String {
        match self {
            Self::Numbered {
                number,
                total,
                display,
            } => numbered_filename(*number, *total, display, extension),
            Self::Template { stem } => format!("{stem}{extension}"),
        }
    }

    pub fn find_existing(&self, directory: &Path, extension: &str) -> Option<PathBuf> {
        match self {
            Self::Numbered { display, .. } => find_existing(directory, display, extension),
            Self::Template { stem } => {
                Some(directory.join(format!("{stem}{extension}"))).filter(|p| p.is_file())
            }
        }
    }
}

pub fn track_display_name(artist: &str, title: &str) -> String {
    let safe_artist = sanitize(Some(artist), "Unknown Artist");
    let safe_title = sanitize(Some(title), "Untitled");
    let (lower_title, lower_artist) = (safe_title.to_lowercase(), safe_artist.to_lowercase());
    if lower_title.starts_with(&format!("{lower_artist} - "))
        || lower_title.starts_with(&format!("{lower_artist} \u{2013} "))
    {
        return safe_title;
    }
    sanitize_filename(
        Some(&format!("{safe_artist} - {safe_title}")),
        180,
        "Untitled",
    )
}

pub fn numbered_filename(
    track_number: usize,
    total_tracks: usize,
    display_name: &str,
    extension: &str,
) -> String {
    let width = total_tracks.max(1).to_string().len().max(2);
    format!("{track_number:0width$}. {display_name}{extension}")
}

pub fn playlist_folder_name(artist: &str, title: &str) -> String {
    sanitize(Some(&format!("{artist} - {title}")), "SoundCloud Playlist")
}

pub fn find_existing(directory: &Path, display_name: &str, extension: &str) -> Option<PathBuf> {
    let suffix = format!("{display_name}{extension}");
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| {
                    name.ends_with(&suffix)
                        && NUMBER_PREFIX.is_match(name)
                        && name.len() - suffix.len() <= 8
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_avoids_duplicated_artist() {
        assert_eq!(track_display_name("DJ", "Song"), "DJ - Song");
        assert_eq!(track_display_name("DJ", "dj - Song"), "dj - Song");
        assert_eq!(
            track_display_name("DJ", "DJ \u{2013} Song"),
            "DJ \u{2013} Song"
        );
        assert_eq!(track_display_name("", "a/b"), "Unknown Artist - a b");
    }

    #[test]
    fn numbering_width_follows_total() {
        assert_eq!(numbered_filename(3, 9, "x", ".mp3"), "03. x.mp3");
        assert_eq!(numbered_filename(7, 150, "x", ".m4a"), "007. x.m4a");
    }

    #[test]
    fn finds_existing_regardless_of_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("12. DJ - Song.mp3"), b"x").expect("escrita");
        std::fs::write(dir.path().join("DJ - Song.mp3"), b"x").expect("escrita");
        let found = find_existing(dir.path(), "DJ - Song", ".mp3").expect("encontrado");
        assert_eq!(
            found.file_name().and_then(|n| n.to_str()),
            Some("12. DJ - Song.mp3")
        );
        assert!(find_existing(dir.path(), "Other", ".mp3").is_none());
        assert!(find_existing(&dir.path().join("missing"), "DJ - Song", ".mp3").is_none());
    }

    #[test]
    fn templates_are_validated_and_rendered_safely() {
        assert!(validate_template("{number} - {artist} - {title}").is_ok());
        assert!(validate_template("{artist}").is_err(), "sem title/id");
        assert!(
            validate_template("{title} {bpm}").is_err(),
            "campo desconhecido"
        );
        assert!(validate_template("   ").is_err());
        let values = TemplateValues {
            number: "03",
            artist: "AC/DC",
            title: "Back../../In Black",
            album: "Hits",
            year: Some(1980),
            id: 42,
            genre: None,
            uploader: "acdc",
        };
        assert_eq!(
            render_template("{year} {artist} - {title} [{id}]{genre}", &values),
            "1980 AC DC - Back.. .. In Black [42]"
        );
    }

    #[test]
    fn template_naming_matches_exact_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let naming = FileNaming::Template {
            stem: "Song [42]".into(),
        };
        assert_eq!(naming.file_name(".mp3"), "Song [42].mp3");
        assert!(naming.find_existing(dir.path(), ".mp3").is_none());
        std::fs::write(dir.path().join("Song [42].mp3"), b"x").expect("escrita");
        assert!(naming.find_existing(dir.path(), ".mp3").is_some());
    }

    #[test]
    fn template_renders_the_uploader_field() {
        let values = TemplateValues {
            number: "01",
            artist: "Artista",
            title: "Faixa",
            album: "Album",
            year: None,
            id: 7,
            genre: None,
            uploader: "Canal",
        };
        assert_eq!(
            render_template("{uploader} - {title}", &values),
            "Canal - Faixa"
        );
    }

    #[test]
    fn existing_file_prefix_is_limited_to_a_track_number() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("1234567. Artista - Faixa.mp3"), b"x").expect("escrita");
        assert!(
            find_existing(dir.path(), "Artista - Faixa", ".mp3").is_none(),
            "prefixo de 9 caracteres nao e numero de faixa"
        );
        std::fs::write(dir.path().join("123456. Artista - Faixa.mp3"), b"x").expect("escrita");
        assert!(find_existing(dir.path(), "Artista - Faixa", ".mp3").is_some());
    }
}
