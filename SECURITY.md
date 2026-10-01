# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| Latest published release | Yes |
| Earlier versions | No. Update to the latest release |

## Reporting a vulnerability

Do not open public issues for security problems. Report them privately through
[GitHub Security Advisories](https://github.com/Isllanrx/Perseus/security/advisories/new).

If possible, include:

- the Perseus version and your operating system;
- steps to reproduce;
- the expected impact;
- the log of the run (`%LOCALAPPDATA%\Perseus\data\logs`, or `--log-format json --log-file perseus.log` on the CLI),
  with personal data removed.

You will get an acknowledgement within 7 days. Fixes for confirmed issues ship in a new release, and the advisory is
disclosed once the fix is available.

## Scope

In scope, for example:

- running the app's IPC commands from content other than its own interface (CSP, Tauri capabilities);
- code execution, or reading or writing files outside the destination folder (path traversal through file names);
- downloading bytes from hosts outside the SoundCloud media allowlist;
- unbounded resource consumption (file size, HLS manifests, API responses);
- data leaking into logs;
- problems with the distributed installer or the release process (checksums, provenance).

Out of scope:

- flaws in SoundCloud itself or changes to its public API;
- tracks unavailable because of DRM, SoundCloud Go+ or regional restrictions. Perseus does not circumvent protections
  by design;
- SmartScreen or Defender warnings about the unsigned installer, as long as its SHA-256 matches the release's
  `SHA256SUMS`.
