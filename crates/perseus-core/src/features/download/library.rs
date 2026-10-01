use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use crate::shared::config::ARCHIVE_FILE;
use crate::shared::error::{Error, Result};
use crate::shared::filesystem::atomic_write;

const ARCHIVE_VERSION: u32 = 1;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

fn to_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|err| Error::InvalidInput(err.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub file: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ArchiveFile {
    #[serde(default)]
    version: u32,
    tracks: BTreeMap<i64, ArchiveEntry>,
}

#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    state: Mutex<(ArchiveFile, bool)>,
}

impl Archive {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(ARCHIVE_FILE);
        let mut file = std::fs::read(&path)
            .ok()
            .and_then(|raw| {
                serde_json::from_slice::<ArchiveFile>(&raw)
                    .inspect_err(|err| tracing::warn!(file = %path.display(), error = %err, "historico ilegivel; recomecando"))
                    .ok()
            })
            .unwrap_or_default();
        let before = file.tracks.len();
        file.tracks
            .retain(|_, entry| is_plain_file_name(&entry.file));
        if file.tracks.len() != before {
            tracing::warn!(file = %path.display(), dropped = before - file.tracks.len(), "historico com caminhos invalidos; entradas ignoradas");
        }
        Self {
            path,
            state: Mutex::new((file, false)),
        }
    }

    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            state: Mutex::new((ArchiveFile::default(), false)),
        }
    }

    pub fn get(&self, track_id: i64) -> Option<ArchiveEntry> {
        lock(&self.state).0.tracks.get(&track_id).cloned()
    }

    pub fn entries(&self) -> Vec<(i64, ArchiveEntry)> {
        lock(&self.state)
            .0
            .tracks
            .iter()
            .map(|(id, entry)| (*id, entry.clone()))
            .collect()
    }

    pub fn record(&self, track_id: i64, entry: ArchiveEntry) {
        let mut state = lock(&self.state);
        if state.0.tracks.get(&track_id) != Some(&entry) {
            state.0.tracks.insert(track_id, entry);
            state.1 = true;
        }
    }

    pub fn remove(&self, track_id: i64) {
        let mut state = lock(&self.state);
        if state.0.tracks.remove(&track_id).is_some() {
            state.1 = true;
        }
    }

    pub fn save(&self) -> Result<()> {
        let mut state = lock(&self.state);
        if !state.1 {
            return Ok(());
        }
        state.0.version = ARCHIVE_VERSION;
        atomic_write(&self.path, &to_json(&state.0)?)?;
        state.1 = false;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LibraryFile {
    #[serde(default)]
    version: u32,
    tracks: BTreeMap<i64, PathBuf>,
}

static LIBRARY_SAVE: Mutex<()> = Mutex::new(());

#[derive(Debug, Default)]
pub struct Library {
    path: Option<PathBuf>,
    known: HashMap<i64, PathBuf>,
    added: Mutex<BTreeMap<i64, PathBuf>>,
}

impl Library {
    pub fn load(path: Option<PathBuf>) -> Self {
        let known = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|raw| serde_json::from_slice::<LibraryFile>(&raw).ok())
            .map(|file| file.tracks.into_iter().collect())
            .unwrap_or_default();
        Self {
            path,
            known,
            added: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn lookup(&self, track_id: i64) -> Option<PathBuf> {
        let added = lock(&self.added).get(&track_id).cloned();
        added
            .or_else(|| self.known.get(&track_id).cloned())
            .filter(|path| path.is_file())
    }

    pub fn record(&self, track_id: i64, path: &Path) {
        if self.path.is_some() {
            lock(&self.added).insert(track_id, path.to_path_buf());
        }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let added = std::mem::take(&mut *lock(&self.added));
        if added.is_empty() {
            return Ok(());
        }
        let _guard = lock(&LIBRARY_SAVE);
        let mut file = std::fs::read(path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<LibraryFile>(&raw).ok())
            .unwrap_or_default();
        file.version = ARCHIVE_VERSION;
        file.tracks.extend(added);
        file.tracks.retain(|_, file_path| file_path.is_file());
        atomic_write(path, &to_json(&file)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(file: &str) -> ArchiveEntry {
        ArchiveEntry {
            file: file.into(),
            title: "t".into(),
            artist: "a".into(),
            duration_ms: Some(1000),
        }
    }

    #[test]
    fn archive_roundtrip_only_writes_when_dirty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let archive = Archive::load(dir.path());
        archive.save().expect("nada a gravar");
        assert!(!dir.path().join(ARCHIVE_FILE).exists());
        archive.record(7, entry("07. a - t.mp3"));
        archive.save().expect("gravado");
        let again = Archive::load(dir.path());
        assert_eq!(again.get(7), Some(entry("07. a - t.mp3")));
        again.remove(7);
        again.save().expect("gravado");
        assert!(Archive::load(dir.path()).get(7).is_none());
    }

    #[test]
    fn archive_ignores_entries_that_escape_the_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut tracks = BTreeMap::new();
        for (id, file) in [
            (1, "../fora.mp3"),
            (2, "sub\\x.mp3"),
            (3, "C:x.mp3"),
            (4, ".."),
            (5, ""),
            (6, "ok.mp3"),
        ] {
            tracks.insert(id, entry(file));
        }
        let raw = serde_json::to_vec(&ArchiveFile { version: 1, tracks }).expect("json");
        std::fs::write(dir.path().join(ARCHIVE_FILE), raw).expect("escrita");
        let ids: Vec<i64> = Archive::load(dir.path())
            .entries()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, vec![6]);
    }

    #[test]
    fn corrupt_archive_starts_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(ARCHIVE_FILE), b"{nao e json").expect("escrita");
        assert!(Archive::load(dir.path()).entries().is_empty());
    }

    #[test]
    fn library_merges_concurrent_writers_and_drops_missing_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let index = dir.path().join("library.json");
        let (a, b) = (dir.path().join("a.mp3"), dir.path().join("b.mp3"));
        std::fs::write(&a, b"a").expect("escrita");
        std::fs::write(&b, b"b").expect("escrita");

        let first = Library::load(Some(index.clone()));
        let second = Library::load(Some(index.clone()));
        first.record(1, &a);
        second.record(2, &b);
        second.record(3, &dir.path().join("sumiu.mp3"));
        first.save().expect("primeiro");
        second.save().expect("segundo");

        let merged = Library::load(Some(index));
        assert_eq!(merged.lookup(1).as_deref(), Some(a.as_path()));
        assert_eq!(merged.lookup(2).as_deref(), Some(b.as_path()));
        assert_eq!(merged.lookup(3), None);
        assert!(Library::disabled().lookup(1).is_none());
    }
}
