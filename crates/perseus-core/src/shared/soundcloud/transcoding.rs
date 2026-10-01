use serde::{Deserialize, Serialize};

use crate::shared::error::{Error, Result};
use crate::shared::soundcloud::models::{Track, Transcoding};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Container {
    Mp3,
    Mp4,
    Ogg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub extension: &'static str,
    pub container: Container,
}

pub fn audio_format_for(mime_type: &str) -> Option<AudioFormat> {
    let essence = mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let (extension, container) = match essence.as_str() {
        "audio/mpeg" => (".mp3", Container::Mp3),
        "audio/mp4" | "audio/aac" | "audio/x-m4a" => (".m4a", Container::Mp4),
        "audio/ogg" => (".opus", Container::Ogg),
        _ => return None,
    };
    Some(AudioFormat {
        extension,
        container,
    })
}

const SUPPORTED_PROTOCOLS: [&str; 2] = ["progressive", "hls"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    #[default]
    Compatible,
    Best,
}

pub fn estimated_kbps(transcoding: &Transcoding) -> u32 {
    let mut parts = transcoding.preset.split('_');
    let codec = parts.next().unwrap_or_default();
    let explicit = parts.find_map(|part| part.strip_suffix('k').and_then(|kbps| kbps.parse().ok()));
    explicit.unwrap_or(if codec == "opus" { 64 } else { 128 })
}

fn rank(transcoding: &Transcoding, quality: Quality) -> Option<(u32, usize, Container)> {
    let format = audio_format_for(&transcoding.format.mime_type)?;
    let protocol = SUPPORTED_PROTOCOLS
        .iter()
        .position(|p| *p == transcoding.format.protocol)?;
    if transcoding.snipped {
        return None;
    }
    let bitrate_key = match quality {
        Quality::Compatible => 0,
        Quality::Best => u32::MAX - estimated_kbps(transcoding),
    };
    Some((bitrate_key, protocol, format.container))
}

pub const REASON_REGION_BLOCKED: &str = "bloqueada para a sua regiao";
pub const REASON_NO_PUBLIC_STREAM: &str = "sem streams publicos (faixa nao reproduzivel sem login)";
pub const REASON_DRM: &str = "protegida por DRM (somente streams criptografados)";
pub const REASON_PREVIEW_ONLY: &str = "apenas previa de 30s disponivel (conteudo SoundCloud Go+)";
pub const REASON_UNSUPPORTED_FORMAT: &str = "nenhum formato de audio suportado";
pub const REASON_HLS_DRM: &str = "stream HLS criptografado (DRM) nao e suportado";
pub const REASON_HLS_BYTE_RANGE: &str = "HLS com byte-range nao e suportado";
pub const REASON_NOT_RETURNED: &str = "nao retornada pela API (privada ou removida)";
pub const REASON_FILTERED: &str = "fora do filtro de duracao";

pub fn reason_code(reason: &str) -> Option<&'static str> {
    Some(match reason {
        REASON_REGION_BLOCKED => "region_blocked",
        REASON_NO_PUBLIC_STREAM => "no_public_stream",
        REASON_DRM | REASON_HLS_DRM => "drm",
        REASON_PREVIEW_ONLY => "preview_only",
        REASON_UNSUPPORTED_FORMAT | REASON_HLS_BYTE_RANGE => "unsupported_format",
        REASON_NOT_RETURNED => "not_returned",
        REASON_FILTERED => "filtered",
        other if other.starts_with("formato nao suportado") => "unsupported_format",
        _ => return None,
    })
}

pub fn choose_transcoding(track: &Track) -> Result<&Transcoding> {
    choose_transcoding_with(track, Quality::Compatible)
}

pub fn choose_transcoding_with(track: &Track, quality: Quality) -> Result<&Transcoding> {
    let unavailable = |reason: &str| Error::StreamUnavailable(reason.to_owned());
    if track.policy.as_deref() == Some("BLOCK") {
        return Err(unavailable(REASON_REGION_BLOCKED));
    }
    let transcodings = track.transcodings();
    if transcodings.is_empty() {
        return Err(unavailable(REASON_NO_PUBLIC_STREAM));
    }
    if let Some(best) = transcodings
        .iter()
        .filter_map(|t| rank(t, quality).map(|key| (key, t)))
        .min_by_key(|(key, _)| *key)
    {
        return Ok(best.1);
    }
    let open: Vec<_> = transcodings
        .iter()
        .filter(|t| !t.format.protocol.contains("encrypted"))
        .collect();
    if open.is_empty() {
        return Err(unavailable(REASON_DRM));
    }
    if open.iter().all(|t| t.snipped) {
        return Err(unavailable(REASON_PREVIEW_ONLY));
    }
    Err(unavailable(REASON_UNSUPPORTED_FORMAT))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn tc(protocol: &str, mime: &str, snipped: bool) -> Value {
        json!({"url": format!("https://api-v2.soundcloud.com/{protocol}"), "snipped": snipped,
               "format": {"protocol": protocol, "mime_type": mime}})
    }

    fn track(transcodings: &[Value], policy: Option<&str>) -> Track {
        serde_json::from_value(json!({"id": 1, "title": "t", "policy": policy, "media": {"transcodings": transcodings}}))
            .expect("track")
    }

    fn reason(track: &Track) -> String {
        choose_transcoding(track)
            .expect_err("deveria falhar")
            .to_string()
    }

    #[test]
    fn prefers_progressive_mp3() {
        let t = track(
            &[
                tc("hls", r#"audio/mp4; codecs="mp4a.40.2""#, false),
                tc("hls", "audio/mpeg", false),
                tc("progressive", "audio/mpeg", false),
            ],
            None,
        );
        assert_eq!(
            choose_transcoding(&t).expect("stream").format.protocol,
            "progressive"
        );
    }

    #[test]
    fn falls_back_to_hls_aac_before_opus() {
        let t = track(
            &[
                tc("hls", r#"audio/ogg; codecs="opus""#, false),
                tc("hls", r#"audio/mp4; codecs="mp4a.40.2""#, false),
            ],
            None,
        );
        assert!(
            choose_transcoding(&t)
                .expect("stream")
                .format
                .mime_type
                .starts_with("audio/mp4")
        );
    }

    #[test]
    fn never_selects_encrypted_streams() {
        let t = track(
            &[
                tc("ctr-encrypted-hls", "audio/mp4", false),
                tc("cbc-encrypted-hls", "audio/mp4", false),
            ],
            None,
        );
        assert!(reason(&t).contains("DRM"));
        let mixed = track(
            &[
                tc("ctr-encrypted-hls", "audio/mp4", false),
                tc("hls", "audio/mpeg", false),
            ],
            None,
        );
        assert_eq!(
            choose_transcoding(&mixed).expect("stream").format.protocol,
            "hls"
        );
    }

    #[test]
    fn rejects_previews_blocked_and_empty() {
        assert!(reason(&track(&[tc("progressive", "audio/mpeg", true)], None)).contains("previa"));
        assert!(
            reason(&track(
                &[tc("progressive", "audio/mpeg", false)],
                Some("BLOCK")
            ))
            .contains("regiao")
        );
        assert!(reason(&track(&[], None)).contains("sem streams"));
    }

    #[test]
    fn maps_mime_types() {
        assert_eq!(
            audio_format_for(r#"audio/ogg; codecs="opus""#).map(|f| f.extension),
            Some(".opus")
        );
        assert!(audio_format_for("audio/mpeg").is_some());
        assert!(audio_format_for("video/mp4").is_none());
    }

    #[test]
    fn reason_codes_cover_every_fixed_reason() {
        for (reason, code) in [
            (REASON_REGION_BLOCKED, "region_blocked"),
            (REASON_NO_PUBLIC_STREAM, "no_public_stream"),
            (REASON_DRM, "drm"),
            (REASON_HLS_DRM, "drm"),
            (REASON_PREVIEW_ONLY, "preview_only"),
            (REASON_UNSUPPORTED_FORMAT, "unsupported_format"),
            (REASON_HLS_BYTE_RANGE, "unsupported_format"),
            (REASON_NOT_RETURNED, "not_returned"),
            ("formato nao suportado: video/mp4", "unsupported_format"),
        ] {
            assert_eq!(reason_code(reason), Some(code), "{reason}");
        }
        assert_eq!(reason_code("tempo esgotado"), None);
    }

    fn preset(protocol: &str, mime: &str, preset: &str) -> Value {
        json!({"url": format!("https://api-v2.soundcloud.com/{preset}"), "preset": preset,
               "format": {"protocol": protocol, "mime_type": mime}})
    }

    #[test]
    fn best_quality_prefers_higher_bitrate() {
        let t = track(
            &[
                preset("progressive", "audio/mpeg", "mp3_1_0"),
                preset("hls", r#"audio/ogg; codecs="opus""#, "opus_0_0"),
                preset("hls", r#"audio/mp4; codecs="mp4a.40.2""#, "aac_160k"),
                preset("hls", r#"audio/mp4; codecs="mp4a.40.2""#, "aac_96k"),
            ],
            None,
        );
        assert_eq!(
            choose_transcoding_with(&t, Quality::Best)
                .expect("stream")
                .preset,
            "aac_160k"
        );
        assert_eq!(
            choose_transcoding_with(&t, Quality::Compatible)
                .expect("stream")
                .preset,
            "mp3_1_0"
        );
    }

    #[test]
    fn estimates_bitrate_from_presets() {
        let parse = |p: &str| -> Transcoding {
            serde_json::from_value(preset("hls", "audio/mpeg", p)).expect("tc")
        };
        assert_eq!(estimated_kbps(&parse("aac_160k")), 160);
        assert_eq!(estimated_kbps(&parse("opus_0_0")), 64);
        assert_eq!(estimated_kbps(&parse("mp3_1_0")), 128);
        assert_eq!(estimated_kbps(&parse("")), 128);
    }

    #[test]
    fn duration_filter_has_its_own_reason_code() {
        assert_eq!(reason_code(REASON_FILTERED), Some("filtered"));
    }
}
