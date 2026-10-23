# ap0dl — typed async Rust anime engine

ap0dl is a Rust-only workspace with a typed, asynchronous provider engine. The
`ap0dl` binary lives in `crates/ap0dl-cli`; all provider, download, proxy and
selection logic lives in `crates/ap0dl-core`. The Python implementation has been
removed after the Rust replacement was verified against live sites. No Python
runtime, package, or Python verification script is required.

The engine is built against live sites, with Aniyomi extension sources used as
behavioural references. Only providers that passed a live
search → details → episodes → stream smoke test are registered.

## Build

```sh
cargo build --release
./target/release/ap0dl --help
```

Requires a current stable Rust toolchain (workspace MSRV: 1.85). The native
Cloudflare fallback builds BoringSSL through `wreq`; on Arch Linux the build
dependencies are:

```sh
pacman -S --needed base-devel cmake clang ffmpeg mpv jq
```

`ffmpeg` is required for HLS remuxing/subtitle merging, and `mpv` for playback.
`jq` and `script` (util-linux) are used by the verification scripts, including
the PTY-based TUI checks.

## Quick start

```sh
# Search every provider; results are fuzzy-ranked locally
ap0dl search "Bocchi the Rock"
ap0dl search "anidb:Bocchi the Rock" --json

# Episode list
ap0dl episodes "anidb:cyberpunk 2" --json

# Interactive: fuzzy anime/episode pickers, then play. After the player exits,
# choose replay, download, next, previous, change quality or another episode.
ap0dl stream "cyberpunk 2"

# Interactive: fuzzy anime picker, then a checkbox episode list with
# "DOWNLOAD ALL EPISODES" and a real-time progress bar
ap0dl download "cyberpunk 2"

# Scripted / non-TTY use
ap0dl stream "cyberpunk 2" --index 1 --range 1 --print-url
ap0dl download "anidb:cyberpunk 2" --range 2 --quality 1080 -d ./downloads
ap0dl download "animetoki:cyberpunk edgerunners" --range 1 --print-url

# Resolve stream JSON for external tools
ap0dl grab "anidb:Bocchi the Rock" --range 1 --quality 1080
ap0dl grab "animeparadise:cyberpunk edgerunners" --range 1
```

Provider prefixes use `provider:query`. A provider URL can also be passed
directly (`https://kisskh.co/drama/...`), in which case the engine selects the
provider by hostname. Search result selection is one-based via `--index`.

Without an explicit provider, searches run concurrently against every registered
provider and results are ranked locally by title tokens, season numbers and edit
distance. `default_provider` only breaks ties between equally good matches.
`stream` and `download` share the same fuzzy anime picker; stream uses a fuzzy
single-episode picker, while download opens a checkbox episode list. Pass
`--index`/`--range` or `--no-interactive` for scripts and pipes.

## Working providers

Live smoke tests were run on 2026-10-22. “1080p” means the provider exposed a
1080p variant in the tested stream; availability can vary by title/episode.

| ID | Source | Search/details/episodes | Stream path | 1080p observed |
|---|---|---|---|---|
| `reanime` | Re:Anime (`reanime.to`) | live | HLS via Flixcloud + local decrypt proxy | **yes** — Bocchi E1: HD-1/HD-2 1080p |
| `animegg` | AnimeGG (`animegg.org`) | live | direct MP4 qualities | **yes** — Frieren S2 E1 1080p |
| `anidb` | AniDB / Anila (`anilab2.amdapi.click`, `anidb.se`) | live | HLS masters; HTML fallback adds 720p MP4s and missing episodes | **yes** — Bocchi E1 1080p; Cyberpunk 2 E1 API 1080p |
| `kisskh` | KissKH (`kisskh.co`) | live | HLS/CDN via public episode-key service | 720p observed; source-dependent |
| `animeparadise` | AnimeParadise (`animeparadise.moe`) | live | opaque-token HLS with 1080p variants and subtitles | **yes** — Cyberpunk E1 1080p |
| `animetoki` | AnimeToki (`animetoki.com`) | live | Cloudflare Worker file index; direct MKV | **yes** — Cyberpunk E1 1080p HEVC |

`ap0dl providers` lists the compiled-in sources and
`ap0dl providers --probe <url>` exercises the shared HTTP client.

### Provider notes

- **Re:Anime** resolves `/api/v1/search` → `/api/v1/anime/{slug}` →
  `/api/v1/anime/{slug}/episodes` → `/api/flix/{anilistId}/{ep}`. The Flixcloud
  embed is decrypted through `enc-dec.app`, then served through a local proxy
  that rewrites playlists/keys/segments and strips fake image headers. If the
  upstream metadata endpoint times out, `stream` automatically tries another
  provider with the same title.
- **AnimeGG** parses HTML search/series pages and inline `videoSources` JSON on
  the embed page. Streams are direct MP4s with a `Referer` header.
- **AniDB / Anila** uses the public JSON API for posts/episodes and the play API
  for server iframes. The API occasionally omits middle episodes (it returned
  only 1 and 10 for *Cyberpunk: Edgerunners 2*), so the provider merges the API
  list with the anidb.se HTML episode list. Web episodes resolve to 720p MP4s
  with the required `Referer`; API episodes keep their 1080p HLS server list.
- **KissKH** uses the public `DramaList` JSON endpoints and the same public Apps
  Script key services as the Aniyomi extension. Episode keys are fetched per
  episode and used to obtain the CDN HLS URL. Encrypted `.txt` subtitles are
  skipped rather than passed to a player as unreadable files.
- **AnimeParadise** uses its public JSON catalogue, resolves an opaque playback
  token, and exposes every resolution from the HLS master. Segments are served
  as `/ts?url=...`, so the stream command adds
  `--demuxer-lavf-o=allowed_segment_extensions=ALL,extension_picky=0` for mpv.
  English/default subtitles are exposed and merged into downloads.
- **AnimeToki** parses the WordPress catalogue and resolves a Cloudflare Worker
  file index. Folder URLs are base64-normalised before the POST because the
  worker redirects percent-encoded paths and converts POST redirects to GET.
  The result is a direct 1080p MKV; the provider’s `container: "mkv"` hint is
  preserved by the downloader.

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

## Legacy providers (explicitly deprecated)

These are not registered because no live stream path was usable. Observed
evidence is recorded in [`docs/legacy-providers.md`](docs/legacy-providers.md)
and is available from the CLI:

```sh
ap0dl providers --legacy
ap0dl providers --legacy --json
```

| ID | Provider | Observation |
|---|---|---|
| `animepahe` | AnimePahe | Cloudflare 403 (`animepahe.pw`); challenge shell on `animepahe.su` |
| `gogoanime` | Gogoanime / Anitaku | `gogoanime.bid` redirected to an HTTP 401 endpoint; `anitaku.pe` is parked |
| `allanime` | AllAnime | API returns Cloudflare 403; earlier source query returned `AA_CRYPTO_MISSING` |
| `aniwave` | AniWave / 9anime | redirects to parked `ww38.aniwave.to` |

Additional sites probed on 2026-10-22 — AniHQ, Kimoitv, Anikuro and HStream —
are also documented with observed responses. Repairing any legacy entry requires
a new live endpoint and a fresh end-to-end smoke test before registration.

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
merge_subtitles = true
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
| `AP0DL_REANIME_BASE`, `AP0DL_ANIMEGG_BASE` | provider bases |
| `AP0DL_ANIDB_BASE`, `AP0DL_ANIDB_PLAY`, `AP0DL_ANIDB_WEB` | AniDB API/play/web bases |
| `AP0DL_KISSKH_BASE` | KissKH base |
| `AP0DL_ANIMEPARADISE_BASE`, `AP0DL_ANIMEPARADISE_API`, `AP0DL_ANIMEPARADISE_STREAM` | AnimeParadise bases |
| `AP0DL_ANIMETOKI_BASE`, `AP0DL_ANIMETOKI_CLOUD` | AnimeToki bases |

`request_delay_ms` is a CDN-reliability setting, not a completion requirement.

## Streaming

`stream` resolves one episode by default; pass `--range` for a range or `:` for
every episode. In a terminal it shows a fuzzy anime picker and then a fuzzy
episode picker. Subtitle tracks are downloaded eagerly only for the
default/English tracks (up to four); the player still receives every selected
track.

When the player exits on the normal interactive path (no `--range`), a second
fuzzy menu appears:

- **Replay this episode**
- **Download this episode instead** — uses the configured `download_dir` and
  shows the normal download progress bar
- **Next** / **Previous** — moves through the provider's episode list
- **Change quality** — lists the qualities exposed by the current stream, then
  replays at the chosen one
- **Pick another episode** — opens the fuzzy episode list again
- **Quit**

Next/previous and quality changes keep working across episodes. The menu is
skipped for scripted/non-TTY runs and explicit `--range` playlists.

Provider selection is health-aware. When a provider’s stream endpoint fails,
times out, or returns no streams, that provider is demoted for ten minutes via
`/tmp/ap0dl-provider-health`, the same title is searched on the other live
providers, and the next provider that has the exact episode is used. Successful
providers are marked healthy again. This means one dead upstream does not block
playback or download.

On WSL, the default `player = "mpv"` resolves to Windows `mpv.exe` when
available. That uses the native Windows GPU stack and avoids the laggy X11/WSLg
window from Linux mpv. If `mpv.exe` is missing, Linux mpv falls back to
`--vo=x11 --hwdec=no` unless `--vo` is supplied. Use `--player mpv` to force
Linux mpv, or `--player mpv.exe` to force the Windows player. Subtitle URLs are
passed directly to Windows mpv because it cannot read Linux temporary paths.

## Downloads

`download` shares the fuzzy anime picker with `stream`, then opens a checkbox
episode checklist instead of the single-episode fuzzy picker. Space toggles an
episode, Enter starts the download, and the first row is
`DOWNLOAD ALL EPISODES (N)`. Without `--index`/`--range`, a script gets every
episode for the chosen title, preserving the old non-interactive behaviour.
Downloads show a real-time `indicatif` progress bar with bytes, speed and ETA
(direct files) or segment counts (HLS).

The download engine handles:

- HLS master/media playlists, AES-128 segments (including default IV
  derivation), fake PNG/WebP headers, and Flixcloud XOR-masked segments;
- direct MP4/MKV downloads with resume/`Range` retry when CDNs reset;
- optional subtitle files and ffmpeg remuxing/merging;
- provider container hints, so AnimeToki files are saved as `.mkv` instead of
  being renamed to `.mp4`;
- `--no-merge` to keep raw HLS output and skip remux/subtitle merging;
- `--print-url` (and `--json` with `--print-url`) for scripting.

Verified live outputs:

- AniDB *Cyberpunk 2* E02: 164,375,734-byte MP4, H.264/AAC 1280×720.
- AnimeParadise *Cyberpunk* E01: 331,133,283-byte MKV, H.264/AAC 1920×1080
  plus merged SubRip subtitles.
- AnimeToki *Cyberpunk* E01: 506,827,883-byte MKV, HEVC 1920×1080, dual AAC and
  embedded subtitle tracks.

## Manga plugin (optional)

Manga support is a separate crate, `crates/ap0dl-manga`, built on the same
engine pieces and modelled on Aniyomi's manga sources. It is compiled only with
the `manga` feature:

```sh
cargo build --release --features ap0dl-cli/manga
ap0dl manga search "bocchi the rock"
ap0dl manga read "mangadex:bocchi the rock"
ap0dl manga download "mangadex:bocchi the rock" --range 1-3
```

Sources, commands, configuration and verification are documented in
[`docs/manga.md`](docs/manga.md).

## Native Cloudflare handling

`crates/ap0dl-core/src/cloudflare.rs` runs a second HTTP client built on `wreq`
with Chrome 137 TLS/HTTP2 impersonation. The shared request layer invokes it
after a 403/503 response or a 200 response carrying a challenge shell. It does
**not** execute JavaScript, drive a browser or solve Turnstile; sites requiring
those remain explicitly blocked. No Selenium/Playwright dependency exists in the
workspace.

Example probe:

```sh
ap0dl providers --probe 'https://anihq.cc/search/?q=cyberpunk'
```

## Architecture

```text
crates/ap0dl-core
  config, http, cloudflare, models, provider trait/registry, engine, legacy
  providers/{reanime,animegg,anidb,kisskh,animeparadise,animetoki}
  download/{mod,hls}, proxy, quality, ranges, player, aniskip, util
crates/ap0dl-cli
  search, episodes, grab, stream, download, providers, config
  commands/{interactive,health,fallback}
```

The provider trait is async and receives the shared `Http` client, so retries,
cookies, cache, Cloudflare fallback and request pacing are uniform. Streams carry
quality, audio, server, subtitle, header, HLS/direct kind, container hint and
optional Flixcloud metadata. The picker and health/fallback logic are CLI
modules shared by both playback and downloading.

## Verification

The ordered implementation checklist is in
[`docs/goal-checklist.md`](docs/goal-checklist.md), mirrored to
`/root/goals/ap0dl-rust-rewrite.md`. Live evidence for the current goal is in
[`docs/evidence/live-smoke-2026-10-22.md`](docs/evidence/live-smoke-2026-10-22.md).

```sh
cargo build --workspace
cargo test --workspace           # 61 passed
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/verify-improvements.sh
./scripts/verify-goal.sh
```

`verify-improvements.sh` checks the live AniDB 10-episode list, AnimeParadise and
AnimeToki 1080p streams, the download checklist, the post-playback menu, health
ordering, automatic fallback and the Cloudflare probe. `verify-goal.sh` runs the
full local harness
and all six live provider smokes. The test suite covers quality selection
(including the 1080p preference), episode ranges, HLS playlist/AES parsing,
fake-header/XOR stripping, proxy URL handling, Cloudflare challenge detection and
provider parsers.

## Rust-only migration

The Python package, `pyproject.toml`, `poetry.lock`, Python README and Python
verification scripts were deleted after the Rust engine passed build, test and
live download checks. The old Python code remains available in git history for
reference, but this working tree is Rust-only.

The configuration format changed from YAML to TOML; start from
`ap0dl config --sample`. The CLI equivalents are:

| Old concept | Rust command |
|---|---|
| search | `ap0dl search "title"` |
| episode list | `ap0dl episodes "query" --json` |
| resolve/play | `ap0dl stream "query"` |
| download | `ap0dl download "query" --range 1-3` |
| provider status | `ap0dl providers` |
| config inspection | `ap0dl config --sample` |

## Scope note

This is a sandboxed research/porting effort against public sites. Endpoint
availability, Cloudflare rules and CDN behaviour change frequently; a provider is
considered working only after the live smoke tests above. Deprecated or blocked
sites are documented rather than silently faked.