# Prioritized backlog

Main source: a comparative analysis of scdl-org/scdl, yt-dlp, yaaaarn/downcloud, imthaghost/scdl,
jerry08/SoundCloudExplode and NotTobi/soundcloud-dl (2026-09-29). Effort: S (<1 day), M (1–3 days), L (>3 days).

| # | Item | Priority | Effort | Why |
|---|---|---|---|---|
| 1 | ~~Original file download~~ | Dropped | — | `/tracks/{id}/download` returns 401 without login; the product is anonymous. |
| 2 | ~~User profiles: uploads, likes, reposts, playlists~~ Done in v1.0 | High | M | Profile URLs returned "unsupported resource"; scdl and downcloud download entire catalogs. A new `features/profile.rs` slice reusing `download`. |
| 3 | ~~Archive by track ID + sync mode~~ Done in v1.0 (sync moves to `Removed/`, never deletes) | High | M | Reuse depended on the file name; renaming a title broke it. A per-ID archive (local file) makes idempotency robust; sync handles tracks that left the playlist (opt-in). |
| 4 | ~~File name templates~~ Done in v1.0 | Medium | S | A common request (scdl `--name-format`). Validate placeholders and keep `sanitize_filename` + `ensure_within`. |
| 5 | ~~HLS with parallel segments~~ | Done in v1.0 | — | 6 segments per track, written in order, per-segment retry. |
| 6 | ~~`.m3u` generation per playlist~~ Done in v1.0 (`.m3u8`) | Medium | S | scdl generates one; makes importing into players easier. |
| 7 | Optional remuxing with ffmpeg (fragmented MP4 from HLS AAC) and format conversion | Medium | M | Fixes the fMP4 duration heuristic (see the review) and enables FLAC/Opus on demand. ffmpeg must stay optional, detected on the PATH. |
| 8 | ~~SHA-pinned actions~~ | Done | — | Every action pinned by SHA; Dependabot keeps them current. |
| 9 | Authenticode signing of the installer | High (distribution) | S technically / certificate cost | The only definitive mitigation for SmartScreen/Defender. Depends on the maintainer buying a certificate. |
| 10 | ~~Installer with shortcut and uninstaller~~ | Done in v1.0 | — | Tauri's NSIS, per user, with the WebView2 bootstrapper. |
| 11 | ~~Interface i18n~~ (the CLI stays in pt-BR) | Done in v1.0 | — | 11 languages in the app; translating the CLI can wait for demand. |
| 12 | Optional persistence of job history | Low | S | Jobs live in memory and disappear when the app closes. |
| 13 | Validate `on.soundcloud.com` short links and long watch runs live | Medium | S | Implemented and tested offline; no evidence yet against the real SoundCloud. |
| 14 | Automatic updates (`tauri-plugin-updater`) with signing | Medium | M | Users currently download every release by hand. |

## SoundCloud in 2026 (endpoints extracted from the web bundle and tested without login, 2026-09-30)

Evidence: `charts?kind=top` (used by scdl/yt-dlp) returns 404; charts are now `music-charts-{region}` playlists listed
in `charts/selections`. Stations and trending are `system-playlist`s under `/discover/sets/<type>:<id>`.
`users/:id/likes` mixes tracks and playlists (flume: 144 tracks + 7 playlists; only tracks are downloaded today).
Comments carry a `timestamp` in ms; `waveform_url` provides 1,800 samples; 50% of trending tracks have an ISRC; 26%
are Go+ previews. Every new HLS stream includes `abr_sq` (a master playlist, already supported).

| # | Item | Priority | Effort | Why |
|---|---|---|---|---|
| 15 | Stations and trending: `/discover/sets/artist-stations:ID`, `track-stations:ID`, `trending-by-genre:GENRE` | High | S | The URL validator rejects `:`; the model already accepts `system-playlist`. No competitor downloads stations. |
| 16 | Liked playlists and albums (mixed `users/:id/likes` / `playlist_likes`) | High | S | Current gap: `/likes` ignores liked playlists. Download them as a collection, one folder per playlist. |
| 17 | Chart browser by region and genre (`charts/selections`) | High | M | The old charts endpoint is dead; nobody offers this anymore. |
| 18 | Preservation: detect library tracks removed, made private or moved to Go+ on SoundCloud (`tracks?ids=` in batches of 50) | High | M | A unique insight: "you have the only copy". Metadata only, no download. |
| 19 | Deduplication by ISRC + duration across re-uploads (library) | Medium | S | The same recording uploaded by both label and artist becomes a single local copy. |
| 20 | Artist radar: watch profiles (or a user's `followings`) and download new uploads since the last check | Medium | M | Watch is playlist-only today; no competitor watches artists. |
| 21 | Complete an album from a single track (`tracks/:id/albums`) and "where does this track appear" (`playlists_without_albums`) | Medium | S | Paste a single and the app offers the whole album. |
| 22 | Advanced search: genre facets, `filter.created_at`, `filter.duration`, `filter.license=to_share` (Creative Commons), `search/queries` autocomplete | Medium | S | CC means material cleared for remixing/sampling; no downloader exposes it. |
| 23 | Moments: waveform + a heat map of timed comments; export comments as a synced `.lrc` | Low | M | Players show the comments at the right moment, like lyrics. |
| 24 | Perseus Radio: a station built from several seed artists (`relatedartists` + stations), skipping tracks already in the library | Low | M | Discovery that only downloads what is new. |
| 25 | Artist card: `web-profiles`, verified badge, station, spotlight (pinned tracks) and an `artist.json` in the folder | Low | S | Artist context alongside the files. |
| 26 | Library dashboard: hours, genres, artists, bitrates, disk usage, tracks with an ISRC | Low | M | An overview of the collection; the data is already in the archive/library. |

Dropped (they require login or a subscription): the original file (`/download` 401), Go+ `hq` 256 kbps, personalized
sets (`weekly`, `new-for-you` return 404 anonymously), `/you/insights` and `stats/timeseries` statistics (owner
only).

## Quality and robustness (findings from the test suite, 2026-09-30)

| # | Item | Priority | Effort | Why |
|---|---|---|---|---|
| 27 | Cross-process lock on the `.part` (CLI and app downloading into the same folder at once) | Medium | S | The current lock only works within a process; `fs4` is already a dependency. |
| 28 | Rust core CI on Linux too | Medium | S | Windows only today; paths and file permissions differ. |
| 29 | App build and E2E on macOS/Linux (Tauri's real WebKit) | Low | M | WebKit E2E only runs against the frontend with mocked IPC. |
| 30 | Raise the mutation score of `describe.ts`/`translate.ts` (88%) | Low | S | The remaining survivors are strings and fallback branches. |
| 31 | Raise the mutation score of the online version (`web/src/lib/web`) | Medium | S | A partial run on 2026-09-30 showed ~77% overall once `lib/web` was included, below the 93.4% measured before it existed. |
