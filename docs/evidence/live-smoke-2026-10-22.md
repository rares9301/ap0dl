# Live smoke evidence — 2026-10-22

Captured from `/root/ap0dl` with the Rust `ap0dl` binary. A provider is listed
as working only because the commands below completed with exit code 0 against
the live site on this date. Signed stream URLs are intentionally omitted.

## Six-provider smoke

`scripts/verify-provider.sh` runs search → details → episodes → `grab` and
asserts the expected resolution/kind. All six registered providers passed:

| Provider | Title used | Episodes | Required stream |
|---|---|---:|---|
| Re:Anime | Bocchi the Rock! | 12 | 1080p HLS (`[Sub] HD-1`, `[Sub] HD-2`) |
| AnimeGG | Frieren beyond journeys end season 2 | 10 | 1080p direct MP4 |
| AniDB / Anila | Bocchi the Rock! | 12 | 1080p HLS |
| KissKH | Bocchi the Rock! | 12 | 720p HLS |
| AnimeParadise | Cyberpunk: Edgerunners | 10 | 1080p HLS |
| AnimeToki | Cyberpunk: Edgerunners | 10 | 1080p direct MKV |

Commands:

```sh
cargo build --workspace
for p in reanime animegg anidb kisskh animeparadise animetoki; do
  ./scripts/verify-provider.sh "$p"
done
```

## AniDB episode coverage and download

The AniDB JSON API only listed episodes 1 and 10 for *Cyberpunk: Edgerunners 2*.
The provider now merges the API episode list with the anidb.se HTML episode
list, so the CLI returns episodes 1–10.

```sh
ap0dl episodes 'anidb:cyberpunk 2' --json
# episodes: 10 (E01–E10)

ap0dl download 'anidb:cyberpunk 2' --range 2 --quality best \
  --no-merge --dir /tmp/ap0dl-progress
```

`ffprobe` on `/tmp/ap0dl-progress/Cyberpunk - Edgerunners 2/E02.mp4`:

- H.264 video, 1280×720, AAC audio
- duration 1653.11 s
- 164,375,734 bytes

## AnimeParadise 1080p HLS download

```sh
ap0dl download 'animeparadise:cyberpunk edgerunners' --range 1 \
  --quality 1080 --dir /tmp/ap-dl2 --json
```

JSON result: `/tmp/ap-dl2/Cyberpunk - Edgerunners/E01.mkv`, with
`E01.sub0.vtt`. `ffprobe`:

- H.264 video, 1920×1080, AAC audio, SubRip subtitle
- duration 1444.32 s
- 331,133,283 bytes

The HLS segments are served as `/ts?url=...`; mpv needs
`--demuxer-lavf-o=allowed_segment_extensions=ALL,extension_picky=0`, which the
stream command adds automatically for this host.

## AnimeToki 1080p direct MKV download

```sh
ap0dl download 'animetoki:cyberpunk edgerunners' --range 1 \
  --quality 1080 --dir /tmp/toki-dl --json
```

JSON result: `/tmp/toki-dl/Cyberpunk - Edgerunners (Season 1) Dual Audio HEVC [1080p]/E01.mkv`.
`ffprobe`:

- HEVC video, 1920×1080
- two AAC audio tracks and 37 SubRip subtitle tracks
- duration 1536.43 s
- 506,827,883 bytes

The provider's `container: "mkv"` hint is what keeps the downloaded extension
correct; the direct-download path does not hard-code `.mp4`.

## Shared TUI, progress and fallback

Both `stream` and `download` call the same `interactive.rs` fuzzy anime picker
and episode picker. A PTY run of:

```sh
ap0dl download --print-url --log warn
```

displayed Re:ANIME, AniDB, KissKH, AnimeToki and the other live providers, then
the 10 Cyberpunk episodes. Selecting E01 printed a resolvable URL.

A real download PTY run displayed a live `indicatif` bar with bytes, percentage,
speed and ETA:

```text
E02 [===>----------------------------] 40.08 MiB/156.76 MiB 94.86 MiB/s (1s)
```

Provider health demotion was tested by writing a fresh failure marker for
`reanime`; `ap0dl download 'cyberpunk 2' --print-url --range 2
--no-interactive` then selected AniDB and printed the direct
`https://my.1anime.site/...` MP4. Removing the marker restores normal ranking.

Automatic same-title fallback was tested by pointing AnimeParadise's stream host
at an unreachable local port:

```sh
AP0DL_ANIMEPARADISE_STREAM='http://127.0.0.1:9' \
  ap0dl stream 'animeparadise:cyberpunk edgerunners' \
  --index 1 --range 1 --print-url --no-interactive
```

The command reported “AnimeParadise stream resolution failed … trying another
provider…”, then printed an AniDB stream URL and exited 0.

## Native Cloudflare/TLS fallback

`ap0dl providers --probe 'https://anihq.cc/search/?q=cyberpunk'` first receives
a Cloudflare 403 challenge and then retries with the native `wreq`
Chrome-137 TLS/HTTP2 impersonation client. AniHQ still returns a JavaScript /
Turnstile challenge, so the command fails cleanly with:

```text
TLS impersonation reached a JavaScript/Turnstile challenge (not bypassable without a browser)
```

No Selenium, Playwright or JavaScript execution is used. Sites that can be
satisfied by TLS/JA3/JA4 fingerprinting are expected to pass this path; sites
behind an interactive Turnstile challenge remain explicitly blocked.

## Download checklist and post-playback menu (2026-10-22 follow-up)

A PTY run of:

```sh
ap0dl download 'anidb:cyberpunk 2' --index 1 --print-url
```

displayed the checkbox list:

```text
Select episodes (space toggles, Enter starts download):
> [ ] DOWNLOAD ALL EPISODES (10)
  [ ] Episode 1
  ...
  [ ] Episode 10
```

Toggling the first row and pressing Enter resolved all ten episodes and printed
their URLs. A second run toggled Episode 1 and Episode 2 individually and
printed exactly those two URLs. `--range`, `--no-interactive` and `--print-url`
remain scriptable.

A stream PTY run with a fake player that exits immediately showed the
post-playback fuzzy menu:

```text
Finished Episode 1 — what next? (played 1080p)
> Replay this episode
  Download this episode instead
  Next: Episode 2
  Change quality
  Pick another episode
  Quit
```

- **Next** replayed Episode 2; **Previous** returned to Episode 1.
- **Change quality** listed 1080p/720p/360p, selected 720p and replayed the
  episode at 720p.
- **Download this episode instead** started the normal HLS progress bar and
  wrote to `AP0DL_DOWNLOAD_DIR`.
- `Pick another episode` reuses the fuzzy episode list; `Quit` exits cleanly.

## Automated acceptance

```sh
cargo build --workspace
cargo test --workspace           # 61 passed
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/verify-improvements.sh
```

`verify-improvements.sh` rechecks AniDB's 10-episode list, live 1080p streams
for AnimeParadise and AnimeToki, the download checkbox checklist (including
download-all), the stream post-playback menu (next/quit via a fake player),
health ordering, automatic fallback and the Cloudflare probe.