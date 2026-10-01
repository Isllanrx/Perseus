use std::collections::HashSet;

use m3u8_rs::{KeyMethod, MediaPlaylist, Playlist};
use url::Url;

use crate::shared::config::{MAX_HLS_SEGMENTS, MAX_PLAYLIST_BYTES};
use crate::shared::error::{Error, Result};
use crate::shared::http::{Http, ensure_success, read_limited};
use crate::shared::soundcloud::transcoding::{REASON_HLS_BYTE_RANGE, REASON_HLS_DRM};

async fn load(http: &Http, url: &str) -> Result<Playlist> {
    http.ensure_trusted(url)?;
    let response = ensure_success(http.get(url, &[]).await?, "manifesto HLS")?;
    let body = read_limited(response, MAX_PLAYLIST_BYTES, "manifesto HLS").await?;
    m3u8_rs::parse_playlist_res(&body)
        .map_err(|_| Error::Integrity("manifesto HLS invalido".into()))
}

fn join(base: &Url, uri: &str) -> Result<String> {
    base.join(uri)
        .map(String::from)
        .map_err(|_| Error::Integrity(format!("URI invalida no manifesto HLS: {uri}")))
}

pub async fn segment_urls(http: &Http, url: &str) -> Result<Vec<String>> {
    let mut base =
        Url::parse(url).map_err(|_| Error::Integrity("URL de manifesto invalida".into()))?;
    let media = match load(http, url).await? {
        Playlist::MediaPlaylist(media) => media,
        Playlist::MasterPlaylist(master) => {
            let best = master
                .variants
                .iter()
                .filter(|variant| !variant.is_i_frame)
                .max_by_key(|variant| variant.bandwidth)
                .ok_or_else(|| Error::Integrity("manifesto HLS sem variantes".into()))?;
            let variant_url = join(&base, &best.uri)?;
            base = Url::parse(&variant_url)
                .map_err(|_| Error::Integrity("URL de variante invalida".into()))?;
            match load(http, &variant_url).await? {
                Playlist::MediaPlaylist(media) => media,
                Playlist::MasterPlaylist(_) => {
                    return Err(Error::Integrity("manifesto HLS aninhado".into()));
                }
            }
        }
    };
    let parts = plan_parts(&base, &media)?;
    for part in &parts {
        http.ensure_trusted(part)?;
    }
    Ok(parts)
}

fn plan_parts(base: &Url, media: &MediaPlaylist) -> Result<Vec<String>> {
    let segments = &media.segments;
    if segments.iter().any(|s| {
        s.key
            .as_ref()
            .is_some_and(|key| key.method != KeyMethod::None)
    }) {
        return Err(Error::StreamUnavailable(REASON_HLS_DRM.into()));
    }
    if segments.is_empty() {
        return Err(Error::Integrity("playlist HLS sem segmentos".into()));
    }
    if segments.len() > MAX_HLS_SEGMENTS {
        return Err(Error::Download(format!(
            "playlist HLS com segmentos demais ({})",
            segments.len()
        )));
    }
    if segments
        .iter()
        .any(|s| s.byte_range.is_some() || s.map.as_ref().is_some_and(|m| m.byte_range.is_some()))
    {
        return Err(Error::StreamUnavailable(REASON_HLS_BYTE_RANGE.into()));
    }

    let mut parts = Vec::with_capacity(segments.len() + 1);
    let mut seen_maps = HashSet::new();
    for segment in segments {
        if let Some(map) = &segment.map {
            let map_url = join(base, &map.uri)?;
            if seen_maps.insert(map_url.clone()) {
                parts.push(map_url);
            }
        }
        parts.push(join(base, &segment.uri)?);
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(text: &str) -> MediaPlaylist {
        match m3u8_rs::parse_playlist_res(text.as_bytes()).expect("manifesto") {
            Playlist::MediaPlaylist(media) => media,
            Playlist::MasterPlaylist(_) => panic!("esperava media playlist"),
        }
    }

    fn base() -> Url {
        Url::parse("https://cf-hls-media.sndcdn.com/playlist/abc/playlist.m3u8").expect("url")
    }

    #[test]
    fn orders_init_map_and_segments() {
        let playlist = media(
            "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:10\n#EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXTINF:10.0,\nseg1.m4s\n#EXTINF:10.0,\nhttps://cf-hls-media.sndcdn.com/media/seg2.m4s\n#EXT-X-ENDLIST\n",
        );
        let parts = plan_parts(&base(), &playlist).expect("partes");
        assert_eq!(
            parts,
            [
                "https://cf-hls-media.sndcdn.com/playlist/abc/init.mp4",
                "https://cf-hls-media.sndcdn.com/playlist/abc/seg1.m4s",
                "https://cf-hls-media.sndcdn.com/media/seg2.m4s",
            ]
        );
    }

    #[test]
    fn refuses_encrypted_streams() {
        let playlist = media(
            "#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXT-X-KEY:METHOD=AES-128,URI=\"https://k.sndcdn.com/key\"\n\
             #EXTINF:10.0,\nseg1.ts\n#EXT-X-ENDLIST\n",
        );
        assert!(matches!(
            plan_parts(&base(), &playlist),
            Err(Error::StreamUnavailable(_))
        ));
    }

    #[test]
    fn refuses_empty_playlists() {
        let playlist = media("#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXT-X-ENDLIST\n");
        assert!(matches!(
            plan_parts(&base(), &playlist),
            Err(Error::Integrity(_))
        ));
    }
}
