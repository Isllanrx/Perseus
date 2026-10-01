use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile as _, TaggedFileExt as _};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::items::Timestamp;
use lofty::tag::{Accessor as _, ItemKey, Tag};

use crate::features::download::models::DownloadJob;
use crate::features::download::validation::lofty_file_type;

fn image_mime(data: &[u8]) -> MimeType {
    if data.starts_with(b"\x89PNG") {
        MimeType::Png
    } else {
        MimeType::Jpeg
    }
}

pub fn apply_tags(path: &Path, job: &DownloadJob, artwork: Option<Vec<u8>>) -> Result<(), String> {
    let mut file = Probe::open(path)
        .map(|probe| probe.set_file_type(lofty_file_type(job.format.container)))
        .and_then(Probe::read)
        .map_err(|err| err.to_string())?;

    let tag_type = file.primary_tag_type();
    if file.primary_tag_mut().is_none() {
        file.insert_tag(Tag::new(tag_type));
    }
    let tag = file.primary_tag_mut().ok_or("formato sem suporte a tags")?;
    tag.set_title(job.title.clone());
    tag.set_artist(
        job.tags
            .artist
            .clone()
            .unwrap_or_else(|| job.artist.clone()),
    );
    tag.set_album(job.album.clone());
    tag.set_track(u32::try_from(job.track_number).unwrap_or(u32::MAX));
    tag.set_track_total(u32::try_from(job.total_tracks).unwrap_or(u32::MAX));
    if let Some(year) = job.tags.year {
        tag.set_date(Timestamp {
            year,
            ..Timestamp::default()
        });
    }
    if let Some(permalink) = &job.permalink_url {
        tag.set_comment(permalink.clone());
    }
    let extras = [
        (ItemKey::Genre, &job.tags.genre),
        (ItemKey::Isrc, &job.tags.isrc),
        (ItemKey::Publisher, &job.tags.label),
        (ItemKey::Composer, &job.tags.composer),
        (ItemKey::CopyrightMessage, &job.tags.copyright),
        (ItemKey::AlbumArtist, &job.tags.album_artist),
    ];
    for (key, value) in extras {
        if let Some(value) = value {
            tag.insert_text(key, value.clone());
        }
    }
    if let Some(data) = artwork {
        tag.remove_picture_type(PictureType::CoverFront);
        let mime = image_mime(&data);
        tag.push_picture(
            Picture::unchecked(data)
                .pic_type(PictureType::CoverFront)
                .mime_type(mime)
                .description("Cover")
                .build(),
        );
    }
    file.save_to_path(path, WriteOptions::new().use_id3v23(true))
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::download::models::TrackTags;
    use crate::shared::soundcloud::transcoding::audio_format_for;

    fn job(dir: &Path) -> DownloadJob {
        let mut job = DownloadJob::for_tests(dir, audio_format_for("audio/mpeg").expect("formato"));
        job.title = "Canção".into();
        job.artist = "uploader".into();
        job.tags = TrackTags {
            artist: Some("Band".into()),
            genre: Some("Techno".into()),
            isrc: Some("DEP960300042".into()),
            label: Some("Label X".into()),
            composer: Some("Composer".into()),
            copyright: None,
            album_artist: Some("Curator".into()),
            year: Some(2024),
        };
        job
    }

    #[test]
    fn writes_id3_tags_and_cover() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("track.mp3");
        let mut frame = vec![0_u8; 417];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        std::fs::write(&path, frame.repeat(200)).expect("escrita");

        let artwork = [b"\x89PNG\r\n\x1a\n".as_slice(), &[0_u8; 32]].concat();
        apply_tags(&path, &job(dir.path()), Some(artwork)).expect("tags");

        let file = lofty::read_from_path(&path).expect("leitura");
        let tag = file.primary_tag().expect("tag id3");
        assert_eq!(tag.title().as_deref(), Some("Canção"));
        assert_eq!(tag.artist().as_deref(), Some("Band"));
        assert_eq!(tag.album().as_deref(), Some("Album"));
        assert_eq!((tag.track(), tag.track_total()), (Some(1), Some(10)));
        assert_eq!(tag.date().map(|d| d.year), Some(2024));
        assert_eq!(
            tag.comment().as_deref(),
            Some("https://soundcloud.com/band/song")
        );
        assert_eq!(tag.pictures().len(), 1);
        assert_eq!(tag.pictures()[0].mime_type(), Some(&MimeType::Png));
        assert_eq!(tag.genre().as_deref(), Some("Techno"));
        assert_eq!(tag.get_string(ItemKey::Isrc), Some("DEP960300042"));
        assert_eq!(tag.get_string(ItemKey::Composer), Some("Composer"));
        assert_eq!(tag.get_string(ItemKey::AlbumArtist), Some("Curator"));
        assert_eq!(tag.get_string(ItemKey::Publisher), Some("Label X"));
    }
}
