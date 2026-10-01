use std::path::Path;

use proptest::prelude::*;
use serde_json::Value;

use crate::features::download::naming::{TemplateValues, render_template, validate_template};
use crate::shared::filesystem::{ensure_within, sanitize, sanitize_filename};
use crate::shared::format::{format_bytes, format_duration, mask_secret};
use crate::shared::soundcloud::models::parse_resource;
use crate::shared::soundcloud::transcoding::audio_format_for;
use crate::shared::soundcloud::urls::{is_valid_client_id, normalize_url};

const WINDOWS_FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn assert_safe_file_name(name: &str) -> Result<(), TestCaseError> {
    prop_assert!(!name.is_empty());
    prop_assert!(
        !name.contains(WINDOWS_FORBIDDEN),
        "separador ou caractere proibido: {name:?}"
    );
    prop_assert!(!name.chars().any(char::is_control), "controle: {name:?}");
    let bidi = |c: char| matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    prop_assert!(!name.chars().any(bidi), "controle bidirecional: {name:?}");
    prop_assert!(
        !name.starts_with([' ', '.']) && !name.ends_with([' ', '.']),
        "borda: {name:?}"
    );
    let stem = name.split('.').next().unwrap_or_default().to_uppercase();
    prop_assert!(
        !RESERVED.contains(&stem.as_str()),
        "nome reservado: {name:?}"
    );
    Ok(())
}

fn template_with_id() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        "[a-z -_.\\[\\]()]{0,6}",
        prop::sample::select(crate::features::download::naming::TEMPLATE_FIELDS.to_vec())
            .prop_map(|field| format!("{{{field}}}")),
    ];
    (
        prop::collection::vec(piece, 0..5),
        any::<prop::sample::Index>(),
    )
        .prop_map(|(mut pieces, at)| {
            let position = at.index(pieces.len() + 1);
            pieces.insert(position, "{id}".to_owned());
            pieces.concat()
        })
}

fn arbitrary_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<f64>()
            .prop_map(|f| serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)),
        ".{0,40}".prop_map(Value::String),
        prop::sample::select(vec![
            "track",
            "playlist",
            "system-playlist",
            "user",
            "progressive",
            "hls"
        ])
        .prop_map(|s| Value::String(s.to_owned())),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
            prop::collection::btree_map(
                prop::sample::select(vec![
                    "kind",
                    "id",
                    "title",
                    "tracks",
                    "media",
                    "transcodings",
                    "url",
                    "format",
                    "protocol",
                    "mime_type",
                    "user",
                    "username",
                    "duration",
                    "policy",
                    "snipped",
                    "track_count",
                    "publisher_metadata",
                    "artwork_url",
                    "x",
                ])
                .prop_map(str::to_owned),
                inner,
                0..10,
            )
            .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_normalize_url_never_panics_and_only_yields_soundcloud(raw in ".{0,300}") {
        if let Ok(url) = normalize_url(&raw) {
            prop_assert!(
                url.starts_with("https://soundcloud.com/") || url.starts_with("https://on.soundcloud.com/"),
                "{raw:?} -> {url:?}"
            );
            prop_assert!(!url.contains(".."), "{url:?}");
            prop_assert!(!url.contains('@'), "{url:?}");
            prop_assert!(!url[8..].contains(':'), "porta ou esquema embutido: {url:?}");
        }
    }

    #[test]
    fn prop_normalize_url_is_idempotent(raw in ".{0,200}") {
        if let Ok(url) = normalize_url(&raw) {
            prop_assert_eq!(normalize_url(&url).ok(), Some(url));
        }
    }

    #[test]
    fn prop_valid_soundcloud_paths_are_accepted(
        user in "[a-z0-9][a-z0-9_-]{0,20}",
        slug in "[a-z0-9][a-z0-9_-]{0,30}",
        prefix in prop::sample::select(vec!["https://", "http://", "https://www.", "https://m.", ""]),
    ) {
        let url = normalize_url(&format!("{prefix}soundcloud.com/{user}/{slug}")).expect("link valido");
        prop_assert_eq!(url, format!("https://soundcloud.com/{user}/{slug}"));
    }

    #[test]
    fn prop_hosts_that_only_contain_soundcloud_are_rejected(
        prefix in "[a-z]{1,10}",
        suffix in prop::sample::select(vec![".evil.com", ".com.br.evil", "x.net"]),
    ) {
        let lookalike = format!("https://soundcloud.com{suffix}/a/b");
        let glued = format!("https://{prefix}soundcloud.com/a/b");
        prop_assert!(normalize_url(&lookalike).is_err());
        prop_assert!(normalize_url(&glued).is_err());
    }

    #[test]
    fn prop_client_id_validation_never_panics(raw in ".{0,64}") {
        let _ = is_valid_client_id(&raw);
    }

    #[test]
    fn prop_sanitized_names_are_safe_on_windows(raw in ".{0,300}", max in 1_usize..200) {
        let name = sanitize_filename(Some(&raw), max, "Untitled");
        assert_safe_file_name(&name)?;
        prop_assert!(name.chars().count() <= max.max("Untitled".chars().count() + 1));
    }

    #[test]
    fn prop_sanitize_is_idempotent(raw in ".{0,200}") {
        let once = sanitize(Some(&raw), "Untitled");
        prop_assert_eq!(sanitize(Some(&once), "Untitled"), once);
    }

    #[test]
    fn prop_sanitized_names_never_escape_the_base(raw in ".{0,200}") {
        let base = std::env::temp_dir().join("perseus-prop-base");
        let name = sanitize(Some(&raw), "Untitled");
        let target = ensure_within(&base, &base.join(&name)).expect("nome saneado fica dentro da base");
        prop_assert!(target.starts_with(&base));
    }

    #[test]
    fn prop_traversal_attempts_are_blocked(depth in 1_usize..6, leaf in "[a-z]{1,8}") {
        let base = std::env::temp_dir().join("perseus-prop-base");
        let escape = format!("{}{leaf}", "../".repeat(depth));
        prop_assert!(ensure_within(&base, &base.join(escape)).is_err());
    }

    #[test]
    fn prop_templates_render_safe_distinct_names_when_they_include_the_id(
        template in template_with_id(),
        title in ".{0,80}",
        artist in ".{0,40}",
        a in 1_i64..i64::MAX,
        b in 1_i64..i64::MAX,
    ) {
        prop_assume!(a != b);
        prop_assert!(validate_template(&template).is_ok(), "gerador so produz templates validos: {template:?}");
        let render = |id| render_template(&template, &TemplateValues {
            number: "01", artist: &artist, title: &title, album: "Album", year: Some(2024), id,
            genre: None, uploader: &artist,
        });
        let (first, second) = (render(a), render(b));
        assert_safe_file_name(&first)?;
        if first.chars().count() < 190 && second.chars().count() < 190 {
            prop_assert_ne!(first, second, "{{id}} garante nomes distintos");
        }
    }

    #[test]
    fn prop_template_validation_never_panics(template in ".{0,300}") {
        let _ = validate_template(&template);
    }

    #[test]
    fn prop_formatters_never_panic(ms in any::<Option<i64>>(), bytes in any::<u64>()) {
        prop_assert!(!format_duration(ms).is_empty());
        prop_assert!(!format_bytes(bytes).is_empty());
    }

    #[test]
    fn prop_mask_never_reveals_a_client_id(secret in "[A-Za-z0-9]{32}") {
        let masked = mask_secret(&secret);
        prop_assert!(!masked.contains(&secret[4..28]), "{masked}");
        prop_assert!(masked.chars().filter(char::is_ascii_alphanumeric).count() <= 8);
    }

    #[test]
    fn prop_parse_resource_never_panics_on_arbitrary_json(value in arbitrary_json()) {
        let _ = parse_resource(value);
    }

    #[test]
    fn prop_archive_loading_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..2048),
        json in arbitrary_json(),
    ) {
        use crate::features::download::library::Archive;
        for content in [bytes, json.to_string().into_bytes()] {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(dir.path().join(crate::shared::config::ARCHIVE_FILE), &content).expect("escrita");
            let archive = Archive::load(dir.path());
            for (_, entry) in archive.entries() {
                prop_assert!(!entry.file.contains(['/', '\\', ':']), "{:?}", entry.file);
            }
        }
    }

    #[test]
    fn prop_audio_validation_rejects_random_bytes_without_panicking(
        bytes in prop::collection::vec(any::<u8>(), 0..4096),
        mime in prop::sample::select(vec!["audio/mpeg", "audio/mp4", "audio/ogg"]),
    ) {
        use crate::features::download::validation::is_valid_audio;
        let format = audio_format_for(mime).expect("formato suportado");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("x{}", format.extension));
        std::fs::write(&path, &bytes).expect("escrita");
        prop_assert!(!is_valid_audio(Path::new(&path), format), "bytes aleatorios nao sao audio valido");
    }
}
