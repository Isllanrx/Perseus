# Ignore robots.txt

**Status:** Active (2026-09-29)

## Context
Perseus does not consult robots.txt. In v0.2, Scrapy was only used against the `api-v2.soundcloud.com` JSON API
(resolve and track batches), consumed exactly as the public web player consumes it; there is no navigation or
crawling of HTML pages. Calls made through `requests` never consulted robots.txt either.

## Decision
`ROBOTSTXT_OBEY = False`, fixed in `crawler/settings.py`.

## Alternatives considered
- **Obey robots.txt**: it would block the API endpoints the player itself uses and make the project unworkable.

## Consequences
- Responsibility for use lies with the user (README/legal notice).
- Mitigations for the impact on the service: AutoThrottle, `DOWNLOAD_DELAY` with jitter, per-domain concurrency
  limits, retries that honor `Retry-After`, batches of 50 IDs.

## Update (2026-09-30, Rust rewrite)
Without Scrapy, the decision still holds by construction: the Rust client (reqwest) never consults robots.txt and
only reaches the JSON API and the media CDN. Impact mitigations in v1.0: bounded concurrency (`workers` <= 16
tracks, 6 HLS segments per track, 4 track batches at a time), retries with exponential backoff and jitter that honor
`Retry-After` (capped at 60 s), and batches of 50 IDs. Scrapy's AutoThrottle no longer exists.
