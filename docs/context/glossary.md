# Glossary

- **client_id** — the public web-player token (32 alphanumeric characters) embedded in the JS bundles; it does not
  identify a user.
- **stub** — a playlist track that comes with only an `id`; it needs `/tracks?ids=` for metadata and transcodings.
- **transcoding** — a stream variant (protocol + MIME type). `progressive` = a single file; `hls` = segments;
  `*-encrypted-hls` = DRM.
- **snipped** — a 30-second preview (Go+ content).
- **track_authorization** — a per-track token required when requesting the stream URL.
- **settled** — a track the watcher no longer needs to try (completed, unavailable or abandoned).
- **Reporter / Event** — the publisher of a run's typed events: every event becomes a `tracing` log line and is
  delivered to the adapter (CLI or window).
- **job** — a run started from the interface (download or watch), with numbered events and replay through
  `job_events`.
- **part** — a `<name>.part` file being written; it only becomes the final file after validation.
- **run_id** — the correlation id of a run, present in every log line and in the report.
- **Archive** — the `.perseus-archive.json` in each folder; maps track id → file.
- **Library** — the global `library.json`; lets a track already downloaded be copied to another folder without
  network access.
- **Collection** — a profile's `/albums` or `/sets`; several playlists, each in its own folder.
- **Virtual playlist** — likes, uploads, popular tracks, reposts or related tracks handled as a playlist.
- **Sync** — moving whatever left the playlist into `Removed/`.
- **Contract** — a file in `contracts/` that both Rust and the frontend read in their tests; both sides change
  together.
- **Transport retry** — a retry performed by the HTTP layer (5xx, 429, timeout) before the track sees the error;
  a **track retry** redoes the whole transfer with a fresh stream. They are counted separately in the stats.
- **RawServer** — a raw-TCP HTTP test server for failures wiremock cannot simulate.
- **Mutant** — a deliberate code change (cargo-mutants/Stryker); **equivalent** when it does not change the result
  and no test can tell it apart.
- **Sanity / Full** — `test-all.ps1` profiles (quick after a change / with fuzzing, performance, mutation and
  flakiness detection).
