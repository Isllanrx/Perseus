use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile as _, FileType};
use lofty::probe::Probe;

use crate::shared::config::{MIN_AUDIO_BYTES, MIN_AUDIO_SECONDS};
use crate::shared::soundcloud::transcoding::{AudioFormat, Container};

pub fn lofty_file_type(container: Container) -> FileType {
    match container {
        Container::Mp3 => FileType::Mpeg,
        Container::Mp4 => FileType::Mp4,
        Container::Ogg => FileType::Opus,
    }
}

fn has_signature(path: &Path, container: Container) -> bool {
    let mut head = [0_u8; 12];
    let Ok(read) = File::open(path).and_then(|mut file| file.read(&mut head)) else {
        return false;
    };
    let head = &head[..read];
    match container {
        Container::Mp3 => {
            head.starts_with(b"ID3")
                || (head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0)
        }
        Container::Mp4 => head.len() >= 8 && &head[4..8] == b"ftyp",
        Container::Ogg => head.starts_with(b"OggS"),
    }
}

pub fn is_valid_audio(path: &Path, format: AudioFormat) -> bool {
    if std::fs::metadata(path).map_or(true, |meta| meta.len() < MIN_AUDIO_BYTES)
        || !has_signature(path, format.container)
    {
        return false;
    }
    let parsed = Probe::open(path)
        .map(|probe| {
            probe
                .set_file_type(lofty_file_type(format.container))
                .options(ParseOptions::new().read_tags(false).read_cover_art(false))
        })
        .and_then(Probe::read);
    match parsed {
        Ok(file) => {
            let seconds = file.properties().duration().as_secs_f64();
            seconds >= MIN_AUDIO_SECONDS || (format.container == Container::Mp4 && seconds == 0.0)
        }
        Err(err) => {
            tracing::debug!(file = %path.display(), error = %err, "validacao falhou");
            format.container == Container::Mp4
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::soundcloud::transcoding::audio_format_for;

    fn format(mime: &str) -> AudioFormat {
        audio_format_for(mime).expect("formato")
    }

    #[test]
    fn rejects_small_and_foreign_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let small = dir.path().join("small.mp3");
        std::fs::write(&small, b"ID3").expect("escrita");
        assert!(!is_valid_audio(&small, format("audio/mpeg")));

        let html = dir.path().join("error.mp3");
        std::fs::write(&html, [b"<html>".as_slice(), &vec![b' '; 20_000]].concat())
            .expect("escrita");
        assert!(!is_valid_audio(&html, format("audio/mpeg")));
        assert!(!is_valid_audio(&html, format("audio/mp4")));
        assert!(!is_valid_audio(
            &dir.path().join("missing.mp3"),
            format("audio/mpeg")
        ));
    }

    #[test]
    fn accepts_fragmented_mp4_by_signature() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frag.m4a");
        let mut bytes = vec![0, 0, 0, 16];
        bytes.extend_from_slice(b"ftypiso6\0\0\0\0");
        bytes.resize(20_000, 0);
        std::fs::write(&path, bytes).expect("escrita");
        assert!(is_valid_audio(&path, format("audio/mp4")));
    }
}
