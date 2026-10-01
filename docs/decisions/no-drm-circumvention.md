# No DRM circumvention and no disguised previews

**Status:** Active (2026-09-29)

## Context
The goal is to download "any" public track without an account, and the project is published on GitHub. The v0.1
fallback (`transcodings[0]`) could pick `ctr/cbc-encrypted-hls` (DRM on label tracks) and produce garbage, or
download 30-second Go+ previews as if they were the full track.

## Decision
- `choose_transcoding` only accepts open `progressive` and `hls` streams, never `snipped`.
- The HLS fetcher refuses manifests with an `#EXT-X-KEY` other than `NONE`.
- Tracks without an open stream are shown as **unavailable, with a reason** (DRM, Go+ preview, region lock).

## Alternatives considered
- **Decrypt streams**: it would violate anti-circumvention laws (DMCA 1201 and equivalents) and the Terms of
  Service; not viable for a public project.
- **Download previews and flag them in the file name**: it misleads the user and pollutes the library.

## Consequences
- Less coverage than tools that use Go+ OAuth, by choice.
- Clear messages cut down on "track X won't download" issues.
