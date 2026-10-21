# ap0dl — typed async Rust engine

ap0dl is now a Rust workspace with a typed, asynchronous provider engine. The
original Python package remains in the repository for migration/reference, but
the `ap0dl` binary is the Rust implementation in `crates/ap0dl-cli`.

The engine was built against live sites, with the Aniyomi extension sources used
as behavioural references. Only providers that passed a live
search → details → episodes → stream smoke test are registered.

## Build

```sh
cargo build --release
./target/release/ap0dl --help
```

Requires a current stable Rust toolchain (workspace MSRV: 1.85). No Python
runtime is needed for the Rust binary.

## Quick start

```sh
# Search all registered providers (or limit to one with --provider)
ap0dl search "Bocchi the Rock"
ap0dl search "anidb:Bocchi the Rock"
ap0dl episodes "anidb:Bocchi the Rock" --json

# Interactive: fuzzy-filter the anime, then the episode, then play
ap0dl stream "cyberpunk 2"

# Non-interactive / scripted
ap0dl stream "cyberpunk 2" --index 1 --range 1
ap0dl stream "reanime:Bocchi the Rock" --range 1 --print-url

# Resolve streams (JSON for external tools)
ap0dl grab "anidb:Bocchi the Rock" --range 1 --quality 1080
ap0dl grab "kisskh:Bocchi the Rock" --range 1

# Download
ap0dl download "animegg:Bocchi the Rock" --range 1-3 -d ./downloads
```

Provider prefixes use `provider:query`. A provider URL can also be passed
directly (`https://kisskh.co/drama/...`), in which case the engine selects the
provider by hostname. Search result selection is one-based via `--index`.

Without an explicit provider, search requests run concurrently against every
registered provider and results are ranked with local fuzzy matching (title
tokens, season numbers and edit distance). This avoids trusting a site's loose
server-side search. `default_provider` only breaks ties between equally good
matches. In an interactive terminal, `stream` opens a `dialoguer` fuzzy picker;
`--index`/`--range` or `--no-interactive` keeps it scriptable. If `stream` is
run without a query, it prompts for one.

## Working providers

Live smoke tests were run on 2026-10-21. "1080p" means the provider exposed a
1080p variant in the tested stream; availability can vary by title/episode.

| ID | Source | Status | Search/details/episodes | Stream path | 1080p observed |
|---|---|---|---|---|---|
| `reanime` | Re:Anime (`reanime.to`) | stable | live | HLS via Flixcloud + local decrypt proxy | **yes** — Bocchi E1: HD-1/HD-2 1080p |
| `animegg` | AnimeGG (`animegg.org`) | stable | live | direct MP4 qualities | **yes** — Frieren S2 E1 1080p; Bocchi E1 is 720p/480p upstream |
| `anidb` | AniDB / Anila (`anilab2.amdapi.click`, `play.anidb.app`) | stable | live | HLS masters | **yes** — Bocchi E1: 1080p, 720p, 360p |
| `kisskh` | KissKH (`kisskh.co`) | stable | live | HLS/CDN via public episode-key service | 720p observed (Bocchi, Frieren, Solo Leveling); source-dependent |

The default quality is `best`, so the highest advertised stream wins. Explicit
selectors are available:

```sh
ap0dl grab "anidb:Bocchi the Rock" --range 1 --quality 1080
ap0dl grab "anidb:Bocchi the Rock" --range 1 --quality ">=720"
ap0dl grab "reanime:Bocchi the Rock" --range 1 --quality "best[subtitles]/best"
```

The quality parser supports `best`, `worst`, exact resolutions (`1080`, `720p`),
comparators (`>=720`, `<=1080`), audio/attribute filters (`sub`, `dub`,
`subtitles`, `[server=HD-1]`, `[title=r"^\[Dub\]"]`), and slash fallbacks.
The old `best[subtitles]/best` ordering is still available when subtitle
presence matters more than resolution.

### Provider notes

- **Re:Anime** resolves `/api/v1/search` → `/api/v1/anime/{slug}` →
  `/api/v1/anime/{slug}/episodes` → `/api/flix/{anilistId}/{ep}`. The Flixcloud
  embed is decrypted through `enc-dec.app`, then served through a local proxy
  that rewrites playlists/keys/segments and strips fake image headers. This is
  what keeps 1080p HLS playable in normal players and in the downloader.
- **AnimeGG** parses the HTML search/series pages and inline `videoSources`
  JSON on the embed page. Streams are direct MP4s with `Referer` headers.
- **AniDB / Anila** uses the public JSON API for posts/episodes and the play API
  for server iframes. Iframe links are HLS masters with 1080p/720p/360p
  variants; each variant is exposed separately.
- **KissKH** uses the public `DramaList` JSON endpoints and the same public
  Apps Script key services as the Aniyomi extension. Episode keys are fetched
  per episode and used to obtain the CDN HLS URL. Encrypted `.txt` subtitles are
  intentionally skipped rather than passed to a player as unreadable files.

## Legacy providers (explicitly deprecated)

These are not registered in the engine because no live stream path was usable
from this sandbox. Observed evidence is recorded in
[`docs/legacy-providers.md`](docs/legacy-providers.md) and is available from the
CLI:

```sh
ap0dl providers --legacy
```

| ID | Provider | Status | Observation |
|---|---|---|---|
| `animepahe` | AnimePahe | deprecated | Cloudflare 403 (`animepahe.pw`); challenge shell on `animepahe.su` |
| `gogoanime` | Gogoanime / Anitaku | deprecated | `gogoanime.bid` redirected to an HTTP 401 endpoint; `anitaku.pe` is parked |
| `allanime` | AllAnime | deprecated | API returns Cloudflare 403; earlier source query returned `AA_CRYPTO_MISSING` |
| `aniwave` | AniWave / 9anime | deprecated | redirects to parked `ww38.aniwave.to`, no watch/search links |

Repairing any of these requires a new live endpoint and a fresh end-to-end
smoke test before it is moved into the working provider registry.

## Configuration

The Rust engine reads TOML, not the old YAML layout. It searches, in order:

1. `AP0DL_CONFIG` if set;
2. `./ap0dl.toml`;
3. `~/.config/ap0dl/config.toml`;
4. `~/.ap0dl/config.toml`.

Generate a sample and inspect the resolved config:

```sh
ap0dl config --sample > ap0dl.toml
ap0dl config --path
```

Important defaults:

```toml
default_provider = "reanime"
quality = "best"
download_dir = "."
player = "mpv"
ffmpeg = "ffmpeg"
request_delay_ms = 250
timeout_secs = 30
```

Provider endpoints can be overridden in `[providers.*]`, and common settings
have environment variables:

| Variable | Purpose |
|---|---|
| `AP0DL_CONFIG` | config file path |
| `AP0DL_DEFAULT_PROVIDER` | preferred provider when fuzzy scores tie |
| `AP0DL_QUALITY` | default quality selector |
| `AP0DL_DOWNLOAD_DIR` | download root |
| `AP0DL_PLAYER`, `AP0DL_FFMPEG` | player/remuxer binaries |
| `AP0DL_REQUEST_DELAY_MS` | shared request delay |
| `AP0DL_REANIME_BASE`, `AP0DL_ANIMEGG_BASE`, `AP0DL_ANIDB_BASE`, `AP0DL_ANIDB_PLAY`, `AP0DL_KISSKH_BASE` | provider endpoints |

`request_delay_ms` is a reliability setting for shared CDNs; it is not a
completion constraint.

## Streaming

`stream` resolves one episode by default; pass `--range` for a range or `:`
for every episode. In a terminal it uses a fuzzy anime picker and then a fuzzy
episode picker. Subtitle tracks are downloaded eagerly only for the
default/English tracks (up to four) to keep startup fast; the player still
receives every selected track.

On WSL, mpv is launched with `--vo=x11 --hwdec=no` unless `--vo` is supplied,
avoiding the common `VK_ERROR_INITIALIZATION_FAILED` / `vo_x11_init` failure in
software/indirect WSLg or X11 setups. Use `--player-arg=--vo=...` to override.

## Downloads

`download` handles both direct MP4 and HLS streams:

- HLS master/media playlists, AES-128 segments (including the default IV
  derivation), fake PNG/WebP headers, and Flixcloud XOR-masked segments;
- direct MP4 downloads with resume/`Range` retry when CDNs reset connections;
- optional subtitle files and ffmpeg remuxing. If ffmpeg is unavailable, HLS
  downloads are written as playable MPEG-TS (`.ts`) files.

Use `--no-merge` to keep the raw `.ts` output and skip remux/subtitle merging.
The download root is the current directory unless `-d/--dir` or
`download_dir` is configured.

Verified live examples from this workspace:

- Re:Anime HLS download: `/tmp/ap0dl-dl/Bocchi the Rock!/E01.ts`,
  324,500,784 bytes, MPEG-TS sync every 188 bytes.
- AnimeGG direct MP4: `/tmp/ap0dl-mp4/One Piece/E01.mp4`, 65,537,496 bytes,
  starts with `ftypisom`.
- AniDB 360p HLS: `/tmp/ap0dl-anidb/Bocchi the Rock!/E01.ts`, 45,170,008 bytes,
  MPEG-TS sync every 188 bytes.

## Architecture

```text
crates/ap0dl-core
  config, http, models, provider trait/registry, engine
  providers/{reanime,animegg,anidb,kisskh}
  download/{mod,hls}, proxy, quality, ranges, player, aniskip
crates/ap0dl-cli
  search, episodes, grab, stream, download, providers, config
```

The provider trait is async and receives the shared `Http` client, so retries,
cookies, cache and request pacing are uniform. Streams carry quality, audio,
server, subtitle, header, HLS/direct kind, and optional Flixcloud metadata.

## Verification

The ordered implementation checklist is kept in
[`docs/goal-checklist.md`](docs/goal-checklist.md), mirrored to
`/root/goals/ap0dl-rust-rewrite.md`.

```sh
cargo build --workspace
cargo test --workspace
```

The test suite covers quality selection (including the 1080p preference), episode
ranges, HLS playlist/AES parsing, fake-header/XOR stripping, proxy URL
classification, and provider parsers. Live smoke tests are manual because they
depend on external sites; see the provider table above and the commands in
[`docs/legacy-providers.md`](docs/legacy-providers.md).

## Migration from Python

The original Python code is intentionally still present and is not deleted by
the Rust migration. To use the old engine, keep using `runner.py`/the Python
package. To use the new engine, run the Rust binary. The configuration formats
differ (TOML vs YAML); start from `ap0dl config --sample` and port only the
providers/settings you need. The Rust engine does not read the legacy
`ap0dl_config.yml`.

Once the Rust binary is installed and your workflows are verified, the Python
package can be kept as a compatibility surface or removed in a separate,
explicit change.

## Scope note

This is a sandboxed research/porting effort against public sites. Endpoint
availability, Cloudflare rules and CDN behavior change frequently; a provider
is only considered working after the live smoke tests above. The deprecated
providers are documented rather than silently faked.
