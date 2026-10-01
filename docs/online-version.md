# Online version (Vercel)

The same interface as the app runs in the browser, on desktop, Android and iPhone, at
[perseus.isllan.dev](https://perseus.isllan.dev). Audio never passes through the server: the Vercel function only
resolves metadata and signs URLs, and the browser downloads straight from the SoundCloud CDN. The reasoning and
measurements are in [web version on Vercel](decisions/web-version-on-vercel.md).

| | Windows app | Online version |
| --- | --- | --- |
| Tracks, albums, playlists, profiles and search | Yes | Yes |
| Cover art and metadata (ID3v2.3 / MP4) | Yes | Yes (Ogg Opus has no tags) |
| Where files go | A folder you pick | A folder you pick (Chrome/Edge on desktop) or the browser's downloads; playlists as a `.zip` |
| Watch mode, local library, `Removed/`, bandwidth limit | Yes | No (they need an always-on process) |
| Size per download | Unlimited | Up to 1,000 tracks (use **Only download the first**) |

## Deploying to Vercel

1. In Vercel, choose **Add New → Project** and import the repository. Keep **Root Directory** at the root and set
   **Application Preset** to **Other**: `vercel.json` already defines the install step (`npm ci --prefix web`), the
   build (`npm --prefix web run build:web`), the output (`web/dist`), the Rust function `api/perseus.rs` and the
   security headers. Do not import the `web` folder on its own, or the `/api` function is left out.
2. Under **Settings → Build and Deployment**, use Node.js 24.
3. Optionally, under **Settings → Environment Variables**:

   | Variable | Purpose |
   | --- | --- |
   | `SOUNDCLOUD_CLIENT_ID` | A fixed client id (32 characters); skips discovery on every cold start |
   | `PERSEUS_LOG` | Function log filter (default `info`) |

4. Deploy. The first build compiles the Rust function (a few minutes); later builds use the cache.
5. Under **Settings → Domains**, add `perseus.isllan.dev` and create the `CNAME` record
   `perseus → cname.vercel-dns.com` in the `isllan.dev` DNS (if the DNS is already on Vercel, the record is created
   automatically).
6. Under **Firewall → Configure → + New Rule** (Hobby allows one free rate-limit rule):
   - **If** `Request Path` `Starts with` `/api/`
   - **Then** `Rate Limit` · `Fixed Window` · **Time Window** `60s` · **Request Limit** `150` · key `IP`
   - Action `Default (429)` → **Save Rule** → **Review Changes** → **Publish**

## Cost on the Hobby plan

The project is tuned to fit the Hobby plan, which is free and **never bills overages**: when a monthly quota runs
out, Vercel pauses that resource until the 30-day window resets, with no invoice. You are only charged if you move
to Pro.

| Hobby monthly quota | What Perseus does to stay within it |
| --- | --- |
| 1,000,000 function invocations | ~1 per track; `config`, `inspect` and `search` are cached by the CDN and do not re-run the function |
| 4 h of active CPU | Network waits do not count; `SOUNDCLOUD_CLIENT_ID` avoids fetching and parsing the SoundCloud site on every new instance |
| 360 GB-h of memory (fixed 2 GB, ~180 h) | A 30 s `maxDuration`: no stuck request can consume more than that |
| 10 GB of Fast Origin Transfer | Gzipped responses; audio goes straight from the SoundCloud CDN to the browser and never touches Vercel |
| 1,000,000 rate-limit requests | Each visitor stays at ~92 calls/min (a random 400 to 900 ms gap between calls); the function and the Firewall reject anything above 150/min per IP |

Keep an eye on **Usage** in the dashboard. Hobby is meant for personal, non-commercial use.

## Running it locally

```powershell
cargo run -p perseus-vercel          # function at http://127.0.0.1:3000
npm --prefix web run dev:web         # interface at http://localhost:5173 (proxies /api)
```
