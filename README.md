<p align="center">
  <img src="assets/perseus-640.jpg" width="160" alt="Perseus">
</p>

<h1 align="center">Perseus</h1>

<p align="center">
  Download public SoundCloud tracks, albums and playlists, no account required.<br>
  <em>Paste a link, review the tracks, get them with cover art and titles. Then let it watch for new ones.</em>
</p>

<p align="center">
  <a href="https://github.com/Isllanrx/Perseus/actions/workflows/ci.yml"><img src="https://github.com/Isllanrx/Perseus/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/Isllanrx/Perseus"><img src="https://api.scorecard.dev/projects/github.com/Isllanrx/Perseus/badge" alt="OpenSSF Scorecard"></a>
  <a href="https://github.com/Isllanrx/Perseus/releases/latest"><img src="https://img.shields.io/github/v/release/Isllanrx/Perseus?include_prereleases&sort=semver" alt="Release"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-0078D4" alt="Platform">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="MIT">
</p>

<p align="center">
  <a href="https://github.com/Isllanrx/Perseus/releases/latest"><b>Download</b></a> ·
  <a href="https://perseus.isllan.dev"><b>Use it online</b></a> ·
  <a href="https://github.com/Isllanrx/Perseus/issues"><b>Report a bug</b></a>
</p>

<p align="center">
  <img src="assets/interface.png" width="860" alt="Perseus home screen: a field to paste a SoundCloud link, download modes, the Download button and the language selector">
</p>

Perseus downloads public SoundCloud tracks, albums, playlists and profiles and hands you files that are ready to
play: title, artist, album, track number and cover art, organized into one folder per playlist. No account, no
login. In **Watch** mode it checks a playlist at regular intervals and downloads only the new tracks.

The core is written in **Rust** (tokio), the window is a **Tauri 2** app and the interface is **React**: a ~5 MB
installer with no local server and no subprocesses. There is also an [online version](docs/online-version.md) that
runs in the browser, phones included.

> [!IMPORTANT]
> **Educational project.** Perseus only downloads open streams that any anonymous visitor can play on the site. It
> **does not circumvent DRM**: encrypted streams and 30-second SoundCloud Go+ previews are refused by design.
> Respect the [SoundCloud Terms of Use](https://soundcloud.com/terms-of-use) and the artists' copyright. See the
> [Legal notice](#legal-notice).

## Features

- **Everything SoundCloud exposes without login:** tracks, albums, playlists (including private ones shared with a
  `secret_token`), `on.soundcloud.com` short links, profile tabs (uploads, popular, reposts, likes, albums,
  playlists), related tracks and search by name.
- **Know before you download:** the track list shows up front which tracks are unavailable and why (DRM, Go+
  preview, region lock).
- **Quality:** *Compatible* (MP3) or *Best available* (the highest open bitrate, such as 160 kbps AAC).
- **Nothing is downloaded twice:** a per-folder archive, a local library that copies tracks already downloaded to
  another folder without touching the network, and HTTP Range resume when the connection drops.
- **Intact files:** every track is validated before it gets its final name; ID3v2.3, MP4 or Opus tags with ISRC,
  genre, label and cover art at 500x500 or original resolution.
- **Organized output:** file name templates, length filters, an `.m3u8` playlist, and a sync option that moves
  tracks removed from the playlist into `Removed\` instead of ever deleting your files.
- **Fast:** parallel downloads, parallel HLS segments and reused HTTP/2 connections.
- **App and CLI**, an interface in 11 languages (including right-to-left Arabic) and structured JSON logs.

See how it compares with scdl, yt-dlp and other tools in [docs/comparison.md](docs/comparison.md).

## Installation

Requires Windows 10 or 11 (64-bit). The installer fetches Microsoft WebView2 if it is missing.

1. Download `Perseus_<version>_x64-setup.exe` from [Releases](https://github.com/Isllanrx/Perseus/releases).
2. Verify it against the `SHA256SUMS` file published with the release:

   ```powershell
   Get-FileHash .\Perseus_<version>_x64-setup.exe -Algorithm SHA256
   ```

3. Run the installer. It installs for your user only and never asks for administrator rights.

For the command line, extract `perseus-cli.exe` from `perseus-cli-<version>-win-x64.zip` into a folder on your
`PATH`.

> [!NOTE]
> Until the installer is code-signed, SmartScreen may warn you on first launch (**More info** → **Run anyway**).
> Always check the SHA-256 first.

## Usage

### App

1. Paste a link to a track, album, playlist or profile, or type a name and click **Search**.
2. Pick a mode: **Whole playlist**, **This track only** or **Watch**.
3. Under **Settings**, choose the destination folder, simultaneous downloads, quality, file names, length filters
   and the library, `.m3u8` and sync options.
4. Click **Download**. Progress is shown track by track; **Cancel** stops right away and **Open folder** takes you
   straight to the files.

### Command line

```powershell
perseus-cli "https://soundcloud.com/user/sets/playlist"                     # playlist or album
perseus-cli --info "https://soundcloud.com/user/sets/playlist"              # metadata and availability only
perseus-cli "https://soundcloud.com/user/likes" --quality best --max-duration 900
perseus-cli --search "flickermood"                                          # search and pick a result
perseus-cli URL --name-template "{artist} - {title} [{id}]" --sync
perseus-cli URL --watch --interval 120                                      # check every 2 minutes
perseus-cli URL --workers 8 --log-format json --report-json report.json     # automation
```

All options: `perseus-cli --help`. Optional environment variables: `SOUNDCLOUD_CLIENT_ID` (manual client id) and
`PERSEUS_LOG` (log filter, `tracing` syntax).

| Exit code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Partial success: run it again to retry only the missing tracks |
| `2` | Invalid input |
| `3` | API or network failure |
| `130` | Interrupted (Ctrl+C) |

## Where Perseus keeps files

```text
%LOCALAPPDATA%\Perseus\                 app, cache (12 h), data\library.json and data\logs\ (last 7 days)
%USERPROFILE%\Music\Perseus\            default destination
├── <Artist> - <Playlist>\              .perseus-archive.json, <Playlist>.m3u8 and Removed\ (--sync)
└── Single Tracks\                      standalone tracks
```

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| A track is "DRM protected" or "30-second preview only" | Not a bug: Perseus never circumvents DRM or downloads Go+ previews |
| "blocked in your region" or "not returned by the API" | The track is restricted in your country, private or removed |
| "Couldn't connect to SoundCloud" | Check your connection and proxy; if it persists, set `SOUNDCLOUD_CLIENT_ID` and open an issue |
| Some tracks failed (exit code `1`) | Run it again: only what is missing gets downloaded |
| The app won't start | A dialog shows the reason and the log folder; reinstalling also reinstalls WebView2 |

When opening an issue, attach the log of the failed run (`%LOCALAPPDATA%\Perseus\data\logs`, or `--log-file` on the
CLI) and the `run_id` shown in the summary.

## Security

Perseus opens no ports, collects no telemetry, only accepts `soundcloud.com` links, only downloads over HTTPS from
SoundCloud media hosts and never deletes your files. File names are sanitized so no title can escape the
destination folder. Report vulnerabilities privately: [SECURITY.md](SECURITY.md).

## Development

Requires [Rust 1.90+](https://rustup.rs/) (MSVC toolchain on Windows) and [Node.js 24+](https://nodejs.org/).

```powershell
npm ci; npm ci --prefix web          # dependencies
npm run dev                          # app with hot reload
npm run build                        # installer in target\release\bundle\nsis\
pwsh scripts/test-all.ps1            # every suite (-Sanity for a quick pass, -Full adds fuzz, performance, mutation)
```

| Document | Contents |
| --- | --- |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Validation, test suites, standards, CI/CD and releases |
| [docs/context](docs/context) | Architecture, business rules, security, coding standards and glossary |
| [docs/decisions](docs/decisions) | Architecture decisions and the reasoning behind each one |
| [docs/online-version.md](docs/online-version.md) | Deploying to Vercel and staying within the Hobby plan |
| [scripts/README.md](scripts/README.md) / [xtask/README.md](xtask/README.md) | Local automation and the no-comments policy |

## Legal notice

Perseus is published **for educational purposes**: a study in building a downloader with sound engineering. It is
not a commercial product.

- The software is provided **"as is", without warranty of any kind**, under the MIT license, and **the author
  accepts no liability** for damage caused by its use, modification or redistribution.
- Downloading SoundCloud content may violate the service's Terms of Use and the artists' rights. **Whether you use
  it is your decision, and so are the consequences.** Only download what you are entitled to keep.
- Perseus does not circumvent DRM or any other technical protection measure.
- Perseus is not affiliated with, endorsed or sponsored by SoundCloud. SoundCloud and its logo are trademarks of
  SoundCloud Global Limited & Co. KG.

## License

[MIT](LICENSE). Built and maintained by **Isllan Toso** ([isllan.dev](https://isllan.dev/)). Powered by
[Tauri](https://tauri.app/), [tokio](https://tokio.rs/), [reqwest](https://github.com/seanmonstar/reqwest),
[lofty](https://github.com/Serial-ATA/lofty-rs), [m3u8-rs](https://github.com/rutgersc/m3u8-rs),
[React](https://react.dev/) and [Vite](https://vite.dev/).
