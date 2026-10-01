# Vertical slice architecture

**Status:** Active (2026-09-29)

## Context
v0.1 already had per-feature folders, but the presentation layer was horizontal and logic leaked across it:
transcoding selection was duplicated between the spider and the resolver, HTTP headers were repeated in three
modules, and the watcher printed straight through Rich (so the GUI saw nothing). Simple changes touched many files.

## Decision
- `features/<use case>` holds the whole use case: `inspect`, `download` (planning, engines, audio, tagging,
  crawler), `watch` (built on `download` only through the package's public API).
- `shared/` is the cross-cutting kernel with no slice-specific rules: config, errors, http, retry, logging,
  filesystem and the `shared/soundcloud/` integration (auth, client, models, transcoding, urls), used by every
  slice.
- `presentation/` (CLI and web) only translates input and output; it holds no business rules.

## Alternatives considered
- **Layers (domain/application/infrastructure)**: more ceremony for a small app; a new feature would touch 3–4
  layers.
- **Keep the v0.1 structure**: it fixed neither the duplication nor the UI's coupling to Rich.

## Consequences
- A new source (e.g. user profiles) becomes a new slice that reuses `download`.
- Risk: `shared/soundcloud` growing into a god module; keep only API integration there, never slice policy.
