use std::sync::LazyLock;

use regex::Regex;

use crate::shared::config::{MAX_URL_LENGTH, SHORTLINK_HOSTS, WEB_BASE, WEB_HOSTS};
use crate::shared::error::{Error, Result};

static PATH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:/[A-Za-z0-9][A-Za-z0-9_-]*)+$").expect("regex valida"));
static PLAYLIST_CONTEXT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_-]*/sets/[A-Za-z0-9][A-Za-z0-9_-]*$").expect("regex valida")
});
static SECRET_TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^s-[A-Za-z0-9]+$").expect("regex valida"));
static PROFILE_SECTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/([A-Za-z0-9][A-Za-z0-9_-]*)/(tracks|popular-tracks|reposts|likes|albums|sets)$")
        .expect("regex valida")
});
static RELATED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/([A-Za-z0-9][A-Za-z0-9_-]*/[A-Za-z0-9][A-Za-z0-9_-]*)/recommended$")
        .expect("regex valida")
});
static CLIENT_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9]{32}$").expect("regex valida"));

pub fn is_valid_client_id(value: &str) -> bool {
    CLIENT_ID_RE.is_match(value)
}

struct Parts<'a> {
    host: String,
    path: &'a str,
    query: &'a str,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidUrl(message.into())
}

fn split(raw: &str) -> Result<(String, Parts<'_>)> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(invalid("URL vazia."));
    }
    if text.len() > MAX_URL_LENGTH {
        return Err(invalid(format!("URL excede {MAX_URL_LENGTH} caracteres.")));
    }
    let (scheme, rest) = text.split_once("://").unwrap_or(("https", text));
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(invalid(format!("Esquema nao suportado: {scheme}")));
    }
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let (path, query) = tail.split_once('?').unwrap_or((tail, ""));

    if authority.contains('@') {
        return Err(invalid(
            "URL com credenciais ou porta customizada nao e aceita.",
        ));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if let Some(port) = port {
        match port.parse::<u16>() {
            Ok(80 | 443) => {}
            Ok(_) => {
                return Err(invalid(
                    "URL com credenciais ou porta customizada nao e aceita.",
                ));
            }
            Err(_) => return Err(invalid(format!("URL malformada: porta invalida '{port}'"))),
        }
    }
    Ok((
        scheme,
        Parts {
            host: host.to_ascii_lowercase(),
            path,
            query,
        },
    ))
}

fn query_value(query: &str, key: &str) -> Option<String> {
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(name, value)| name == key && !value.is_empty())
        .map(|(_, value)| value.into_owned())
}

pub fn is_shortlink(url: &str) -> bool {
    split(url).is_ok_and(|(_, parts)| SHORTLINK_HOSTS.contains(&parts.host.as_str()))
}

pub fn normalize_url(raw: &str) -> Result<String> {
    let (_, parts) = split(raw)?;
    let path = parts.path.trim_end_matches('/');

    if SHORTLINK_HOSTS.contains(&parts.host.as_str()) {
        if !PATH_RE.is_match(path) {
            return Err(invalid("Shortlink do SoundCloud invalido."));
        }
        return Ok(format!("https://{}{path}", parts.host));
    }

    if !WEB_HOSTS.contains(&parts.host.as_str()) {
        let shown = if parts.host.is_empty() {
            "(vazio)"
        } else {
            parts.host.as_str()
        };
        return Err(invalid(format!(
            "Dominio nao suportado: {shown}. Use um link de soundcloud.com."
        )));
    }
    if path.is_empty() || !PATH_RE.is_match(path) {
        return Err(invalid(
            "Caminho da URL nao corresponde a uma faixa ou playlist do SoundCloud.",
        ));
    }

    let mut kept = Vec::new();
    if let Some(context) =
        query_value(parts.query, "in").filter(|c| PLAYLIST_CONTEXT_RE.is_match(c))
    {
        kept.push(format!("in={context}"));
    }
    if let Some(token) =
        query_value(parts.query, "secret_token").filter(|t| SECRET_TOKEN_RE.is_match(t))
    {
        kept.push(format!("secret_token={token}"));
    }
    let query = if kept.is_empty() {
        String::new()
    } else {
        format!("?{}", kept.join("&"))
    };
    Ok(format!("{WEB_BASE}{path}{query}"))
}

pub fn playlist_context(url: &str) -> Option<String> {
    let normalized = normalize_url(url).ok()?;
    let (_, parts) = split(&normalized).ok()?;
    query_value(parts.query, "in")
        .filter(|context| PLAYLIST_CONTEXT_RE.is_match(context))
        .map(|context| format!("{WEB_BASE}/{context}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSection {
    Uploads,
    PopularTracks,
    Reposts,
    Likes,
    Albums,
    Playlists,
}

pub fn profile_section(url: &str) -> Option<(String, ProfileSection)> {
    let normalized = normalize_url(url).ok()?;
    let (_, parts) = split(&normalized).ok()?;
    let caps = PROFILE_SECTION_RE.captures(parts.path)?;
    let section = match &caps[2] {
        "tracks" => ProfileSection::Uploads,
        "popular-tracks" => ProfileSection::PopularTracks,
        "reposts" => ProfileSection::Reposts,
        "likes" => ProfileSection::Likes,
        "albums" => ProfileSection::Albums,
        _ => ProfileSection::Playlists,
    };
    Some((format!("{WEB_BASE}/{}", &caps[1]), section))
}

pub fn related_track(url: &str) -> Option<String> {
    let normalized = normalize_url(url).ok()?;
    let (_, parts) = split(&normalized).ok()?;
    RELATED_RE
        .captures(parts.path)
        .map(|caps| format!("{WEB_BASE}/{}", &caps[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes() {
        let cases = [
            (
                "https://soundcloud.com/artist/track",
                "https://soundcloud.com/artist/track",
            ),
            (
                "soundcloud.com/artist/sets/mix/",
                "https://soundcloud.com/artist/sets/mix",
            ),
            (
                "https://m.soundcloud.com/artist/track?utm_source=x&si=abc",
                "https://soundcloud.com/artist/track",
            ),
            (
                "http://www.soundcloud.com/a/b?secret_token=s-AbC123",
                "https://soundcloud.com/a/b?secret_token=s-AbC123",
            ),
            (
                "https://soundcloud.com/a/b?in=user/sets/my-mix&fbclid=1",
                "https://soundcloud.com/a/b?in=user/sets/my-mix",
            ),
            (
                "https://soundcloud.com/a/b?in=user%2Fsets%2Fmy-mix",
                "https://soundcloud.com/a/b?in=user/sets/my-mix",
            ),
            (
                "https://soundcloud.com/a/sets/p/s-XyZ9",
                "https://soundcloud.com/a/sets/p/s-XyZ9",
            ),
            (
                "HTTPS://SoundCloud.com:443/a/b#frag",
                "https://soundcloud.com/a/b",
            ),
        ];
        for (raw, expected) in cases {
            assert_eq!(normalize_url(raw).expect(raw), expected, "{raw}");
        }
    }

    #[test]
    fn rejects_untrusted() {
        let long = format!("https://soundcloud.com/{}", "a".repeat(3000));
        let cases = [
            "",
            "https://evil.com/?x=on.soundcloud.com",
            "https://soundcloud.com.evil.com/a/b",
            "https://user:pass@soundcloud.com/a/b",
            "https://soundcloud.com:8443/a/b",
            "ftp://soundcloud.com/a/b",
            "https://soundcloud.com/",
            "https://soundcloud.com/../etc/passwd",
            "https://soundcloud.com/a/%2e%2e/b",
            "javascript:alert(1)",
            "https://soundcloud.com\\@evil.com/a",
            long.as_str(),
        ];
        for raw in cases {
            assert!(
                matches!(normalize_url(raw), Err(Error::InvalidUrl(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn drops_invalid_query_params() {
        assert_eq!(
            normalize_url("https://soundcloud.com/a/b?in=../../x&secret_token=<x>")
                .expect("valida"),
            "https://soundcloud.com/a/b"
        );
    }

    #[test]
    fn extracts_playlist_context() {
        assert_eq!(
            playlist_context("https://soundcloud.com/a/b?in=user/sets/mix").as_deref(),
            Some("https://soundcloud.com/user/sets/mix")
        );
        assert_eq!(playlist_context("https://soundcloud.com/a/b"), None);
    }

    #[test]
    fn detects_profile_sections() {
        let cases = [
            (
                "https://soundcloud.com/kitty-326047409/likes",
                ProfileSection::Likes,
            ),
            (
                "m.soundcloud.com/dj/likes/?utm_source=x",
                ProfileSection::Likes,
            ),
            ("https://soundcloud.com/dj/tracks", ProfileSection::Uploads),
            (
                "https://soundcloud.com/dj/popular-tracks",
                ProfileSection::PopularTracks,
            ),
            ("https://soundcloud.com/dj/reposts", ProfileSection::Reposts),
            ("https://soundcloud.com/dj/albums", ProfileSection::Albums),
            ("https://soundcloud.com/dj/sets", ProfileSection::Playlists),
        ];
        for (url, section) in cases {
            let (profile, found) = profile_section(url).expect(url);
            assert_eq!(found, section, "{url}");
            assert!(
                profile.ends_with("/dj") || profile.ends_with("/kitty-326047409"),
                "{profile}"
            );
        }
        assert_eq!(
            profile_section("https://soundcloud.com/dj/sets/likes"),
            None
        );
        assert_eq!(profile_section("https://soundcloud.com/dj/track"), None);
        assert_eq!(profile_section("https://evil.com/dj/likes"), None);
    }

    #[test]
    fn detects_related_tracks() {
        assert_eq!(
            related_track("https://soundcloud.com/forss/flickermood/recommended").as_deref(),
            Some("https://soundcloud.com/forss/flickermood")
        );
        assert_eq!(
            related_track("https://soundcloud.com/forss/recommended"),
            None
        );
    }

    #[test]
    fn detects_shortlinks() {
        assert!(is_shortlink("https://on.soundcloud.com/AbC123"));
        assert_eq!(
            normalize_url("on.soundcloud.com/AbC123").expect("valida"),
            "https://on.soundcloud.com/AbC123"
        );
        assert!(!is_shortlink("https://soundcloud.com/a/b"));
    }

    #[test]
    fn client_id_format() {
        assert!(is_valid_client_id(&"a".repeat(32)));
        assert!(!is_valid_client_id(&"a".repeat(31)));
        assert!(!is_valid_client_id(&format!("{}!", "a".repeat(31))));
    }

    #[test]
    fn url_length_limit_is_inclusive() {
        let prefix = "https://soundcloud.com/a/";
        let at_limit = format!("{prefix}{}", "b".repeat(MAX_URL_LENGTH - prefix.len()));
        assert_eq!(at_limit.len(), MAX_URL_LENGTH);
        assert!(
            split(&at_limit).is_ok(),
            "URL com exatamente o limite e aceita"
        );
        assert!(
            split(&format!("{at_limit}b")).is_err(),
            "um caractere acima e recusada"
        );
    }

    #[test]
    fn query_values_come_from_the_named_parameter_only() {
        assert_eq!(
            normalize_url("https://soundcloud.com/a/b?utm_source=x&secret_token=s-AbC123")
                .expect("valida"),
            "https://soundcloud.com/a/b?secret_token=s-AbC123"
        );
        assert_eq!(
            query_value("utm=x&in=", "in"),
            None,
            "valor vazio nao conta"
        );
    }
}
