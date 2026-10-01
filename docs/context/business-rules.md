# Business rules

- Only content that can be played anonymously. No login, no OAuth, no DRM.
- Stream preference (Compatible quality, the default): progressive MP3 > other progressive > HLS MP3 > HLS AAC >
  HLS Opus. Best quality: highest estimated bitrate (`preset`), ties broken by the same order.
- The original file (`/download`) requires login: it is out of scope.
- Numbering = position in the full playlist, even when only new tracks are downloaded (watch).
- Folder: `<playlist artist> - <title>`; standalone tracks go to `Single Tracks/`.
- An existing, valid file (even with a different number prefix) is reused, never downloaded again.
- Profile `/albums` and `/sets`: one folder per playlist. Tabs (likes, tracks, reposts, popular-tracks) and related
  tracks become a virtual playlist.
- The per-folder archive (`.perseus-archive.json`) takes precedence over the file name; the global library copies a
  track already downloaded to another folder and rewrites its tags for the new context.
- Sync (opt-in) moves whatever left the playlist into `Removed/`; it never deletes. It is disabled with `limit` or
  a partial download (`only_track_ids`), because the observed list would be incomplete.
- File names are unique per folder: a collision (a template without `{id}`, or a truncated name) gets ` [id]`
  appended; a file belonging to another track is never reused.
- Two simultaneous downloads of the same track into the same folder (in the app) are serialized; the second one
  reuses the first.
- The length filter marks a track as unavailable with the `filtered` code; a name template must include `{title}`
  or `{id}`.
- Disk space is estimated (bitrate x duration) and checked before downloading; running out is the fatal error
  `insufficient_space`.
- Watch: a completed or unavailable track is "settled"; failures are retried in cycles up to
  `WATCH_MAX_TRACK_ATTEMPTS`, then abandoned with an error log.
- Domain errors (404, unsupported resource, invalid URL) end the run without retries; transient failures (network,
  429, 5xx, truncated file) are retried with backoff.
- A report is `ok` only with no fatal error, no failures and no cancellation. Unavailable tracks do not break `ok`.
