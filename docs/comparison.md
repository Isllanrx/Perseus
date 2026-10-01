# Comparison

How Perseus compares with the most widely used SoundCloud download tools, counting only what works **without
logging in**.

| Feature | Perseus | scdl | yt-dlp | SoundCloudExplode | soundcloud-dl (NotTobi) | DownCloud (web) |
| --- | :-: | :-: | :-: | :-: | :-: | :-: |
| Tracks, albums and playlists | Yes | Yes | Yes | Yes | Yes | Yes |
| Likes, reposts and uploads | Yes | Yes | Partial | Partial | No | No |
| Popular and related tracks | Yes | No | Partial | No | No | No |
| Profile albums and playlists, one folder per album | Yes | Partial | No | No | No | No |
| Built-in search | Yes | No | Yes | Yes | No | No |
| Desktop app | Yes | No | No | No | Extension | Web |
| Availability (DRM, Go+, region) shown before downloading | Yes | No | No | No | No | No |
| Parallel downloads | Yes | No | No | No | No | No |
| Quality picked by actual bitrate | Yes | Partial | Yes | Partial | No | No |
| Skips what is already downloaded | Yes | Yes | Yes | No | No | No |
| Watches a playlist and downloads new tracks | Yes | No | No | No | No | No |
| HTTP Range resume | Yes | No | Yes | No | No | No |
| Integrity check before writing | Yes | No | Partial | No | No | No |
| File name templates and length filters | Yes | Yes | Yes | No | No | No |
| `.m3u8` playlist | Yes | Partial | No | No | No | No |
| Reuses tracks from other folders without network | Yes | No | No | No | No | No |
| Syncs removals without deleting files | Yes | No | No | No | No | No |
| Disk space check | Yes | No | No | No | No | No |
| Bandwidth limit | Yes | No | Yes | No | No | No |
| Interface in 11 languages (including RTL) | Yes | No | No | No | No | No |

The original file uploaded by the artist (WAV, FLAC) is only available to logged-in accounts, so it is out of scope
for Perseus, which works anonymously.

## Performance against v0.2

Real measurements (Windows 11, same connection), 11-track / 50 MB playlist:

| | Perseus 0.2 (Python) | Perseus 1.0 (Rust) |
| --- | --- | --- |
| Install | 52.5 MB zip (bundled Python runtime) | 5.2 MB installer |
| Starting a download | a Scrapy subprocess relaunches the whole executable | an async task in the same process |
| HLS | segments in series; one failure restarts the track | parallel segments, retried individually |
| 11-track playlist | — | 2.0 s from the CLI with 8 simultaneous downloads (25 MB/s); 3.4 s from the app with 4 |

## Projects studied

Perseus was built by studying these projects. No code was copied.

| Project | What Perseus learned from it |
| --- | --- |
| [scdl](https://github.com/scdl-org/scdl) — scdl-org | The features users expect: profiles, likes, a download archive, sync |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | The SoundCloud extraction reference most of the ecosystem relies on |
| [downcloud](https://github.com/yaaaarn/downcloud) — yaaaarn | A focus on speed and comparative benchmarking |
| [scdl](https://github.com/imthaghost/scdl) — imthaghost | Batched playlist hydration, HLS segments and explicitly refusing DRM |
| [SoundCloudExplode](https://github.com/jerry08/SoundCloudExplode) — jerry08 | Structuring a client API for tracks, playlists and search |
| [soundcloud-dl](https://github.com/NotTobi/soundcloud-dl) — NotTobi | File name normalization and user-facing quality options |
