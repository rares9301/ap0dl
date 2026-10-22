# Goal: ap0dl — Rust-only engine, shared TUI, expanded live providers

Status: complete — implementation and verification finished 2026-10-22
Created: 2026-10-22
Owner: user
Project: /root/ap0dl
Goal: finish the Python → Rust replacement, expose all AniDB/Anila episodes, add
the same fuzzy picker to `stream` and `download`, expand the live provider set
with two more verified sources, use native Cloudflare/TLS handling, and remove
the Python implementation once Rust parity is proven.

## Objective

Deliver a Rust-only ap0dl binary whose providers all pass a live
search → details → episodes → stream smoke test. `stream` and `download` must share a fuzzy anime picker, download must support
multi-episode checkbox selection (including all), stream must offer a fuzzy
post-playback menu for replay/download/next/previous/quality, downloads must show
real-time progress and produce playable files, AniDB/Anila must expose all available episodes, 1080p
must be preferred when offered, provider selection must be health-aware with
automatic same-title fallback, and Cloudflare-challenged sites must be handled
with native Rust TLS impersonation or documented as blocked. The Python package
must be deleted after the Rust replacement is verified.

## Authorization / constraints

- Owner authorized broad sandbox experimentation and asked for a full-Rust
  result: no Python application, package or verification dependency remains.
- Do not claim a provider works unless a live smoke test succeeded.
- Do not delete the Python implementation before Rust parity was verified.
- Legacy providers must be live-repaired or explicitly deprecated with observed
  HTTP evidence.
- No Selenium, Playwright or browser automation; Cloudflare handling uses a
  native Rust HTTP/TLS client and does not attempt JavaScript/Turnstile bypass.

## Definition of done

A Rust-only workspace builds and tests cleanly, all six registered providers pass
live smoke tests, AniDB exposes the full Cyberpunk 2 episode list, 1080p is
selected when available, download has a multi-select checklist with an all
option, stream has the post-playback control menu, downloads are playable and
show progress, fallback/health ordering is live-tested,
the Cloudflare probe is wired, the Python tree is gone, and documentation plus
the ordered checklist are current.

## Checklist (ordered)

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done.

| # | Work item | Started | Completed | Notes |
|---|-----------|---------|-----------|-------|
| 1 | Recon: live-site probes, Aniyomi/Suzuno behaviour research, provider candidates | [x] | [x] | AniDB web fallback and AnimeParadise/AnimeToki flows identified from live sites and Aniyomi sources |
| 2 | AniDB / Anila episode coverage: merge API list with anidb.se HTML episodes | [x] | [x] | API returned only E01/E10 for Cyberpunk 2; merged list returns E01–E10 |
| 3 | AniDB live stream + download verification | [x] | [x] | Cyberpunk 2 E02 downloaded; H.264/AAC 720p, 1653 s, 164,375,734 bytes |
| 4 | Shared fuzzy anime picker for `stream` and `download` | [x] | [x] | `commands/interactive.rs` backs both; stream uses fuzzy episode picking, download uses a checkbox list |
| 5 | Download progress bars and playable output | [x] | [x] | Direct and HLS byte/segment bars with speed/ETA; MP4 and MKV outputs ffprobe-verified |
| 6 | AnimeParadise provider: search/details/episodes/HLS/1080p/subtitles | [x] | [x] | 10 Cyberpunk episodes; 1080p HLS; downloaded MKV is H.264 1920×1080 + AAC + SubRip |
| 7 | AnimeToki provider: WordPress catalogue + Worker index + direct 1080p MKV | [x] | [x] | 10 episodes; 1080p HEVC MKV download, 506,827,883 bytes, container hint preserved |
| 8 | Additional site audit: AniHQ, Kimoitv, Anikuro, HStream | [x] | [x] | Observed 403/Turnstile, verify redirect, 522 and 404 respectively; documented in `docs/legacy-providers.md` |
| 9 | Native Cloudflare/TLS impersonation fallback | [x] | [x] | `wreq` Chrome-137 retry on 403/503 or a 200 challenge shell; no browser automation |
| 10 | Health-aware provider ordering | [x] | [x] | Failed providers demoted for 600 s via `/tmp/ap0dl-provider-health`; live marker test selected AniDB |
| 11 | Automatic same-title fallback | [x] | [x] | Broken AnimeParadise stream host triggered AniDB fallback and exited 0 |
| 12 | 1080p preference when source offers it | [x] | [x] | `best` selects highest; Re:Anime/AnimeGG/AniDB/AnimeParadise keep 1080p; AnimeToki direct 1080p |
| 13 | CLI scriptability with non-TTY stdin | [x] | [x] | `--index`, `--range`, `--no-interactive`, `--print-url`, `--json`; download shares picker but stays scriptable |
| 14 | Unit tests for quality/ranges/HLS/AES/Cloudflare/provider parsers | [x] | [x] | `cargo test --workspace`: 61 passed, 0 failed |
| 15 | Remove Python implementation after Rust parity | [x] | [x] | Deleted `ap0dl/`, `pyproject.toml`, `poetry.lock`, Python README and Python-only scripts |
| 16 | Docs/evidence: README, provider matrix, legacy page, live smoke page | [x] | [x] | Rust-only README; `docs/legacy-providers.md`; `docs/evidence/live-smoke-2026-10-22.md` |
| 17 | Python-free verification harness, including `verify-improvements.sh` | [x] | [x] | All shell checks use `jq`; live TUI/1080/health/fallback/Cloudflare acceptance script added |
| 18 | First Rust-only verification and checklist sync | [x] | [x] | `cargo fmt --check`, clippy `-D warnings`, build, 56 tests, `verify-improvements.sh` and full `verify-goal.sh` passed |
| 19 | Download multi-select episode checklist with download-all row | [x] | [x] | `MultiSelect` checkpoint list; first row is `DOWNLOAD ALL EPISODES (N)`; multiple episodes can be toggled |
| 20 | Post-playback fuzzy action menu for `stream` | [x] | [x] | Replay, download instead, next, previous, change quality, pick another episode, quit |
| 21 | TUI verification and final checklist sync | [x] | [x] | PTY runs exercised download-all and multi-select, plus replay/next/previous/quality/download actions; 61 tests pass |

## Live smoke evidence (2026-10-22)

All commands used the built Rust binary from this workspace. Raw/short evidence
is summarized in `docs/evidence/live-smoke-2026-10-22.md`; per-provider JSON is
under `docs/evidence/provider-*.txt`.

| Provider | Title | Episodes | Required stream |
|---|---|---:|---|
| Re:Anime | Bocchi the Rock! | 12 | 1080p HLS |
| AnimeGG | Frieren beyond journeys end season 2 | 10 | 1080p direct MP4 |
| AniDB / Anila | Bocchi the Rock! | 12 | 1080p HLS |
| KissKH | Bocchi the Rock! | 12 | 720p HLS |
| AnimeParadise | Cyberpunk: Edgerunners | 10 | 1080p HLS |
| AnimeToki | Cyberpunk: Edgerunners | 10 | 1080p direct MKV |

Provider-specific evidence:

- **AniDB episode coverage** — `ap0dl episodes 'anidb:cyberpunk 2' --json`
  returns 10 episodes (E01–E10). The API-only result had 2.
- **AniDB download** — `ap0dl download 'anidb:cyberpunk 2' --range 2 --no-merge`
  produced `E02.mp4`; ffprobe reports H.264 1280×720 + AAC, 1653.11 s,
  164,375,734 bytes.
- **AnimeParadise** — 1080p HLS verified; downloaded `E01.mkv` is H.264
  1920×1080 + AAC + SubRip, 1444.32 s, 331,133,283 bytes. The stream path adds
  the ffmpeg segment-extension workaround for `/ts?url=...` segments.
- **AnimeToki** — direct 1080p URL resolved; downloaded `E01.mkv` is HEVC
  1920×1080 with two AAC tracks and 37 subtitle tracks, 1536.43 s,
  506,827,883 bytes. The `.mkv` extension is preserved by the container hint.
- **Shared TUI** — PTY runs listed Re:ANIME, AniDB, KissKH, AnimeToki and the
  other live providers. Stream uses the fuzzy single-episode picker; download
  opens a checkbox list.
- **Download checklist** — PTY `download 'anidb:cyberpunk 2' --index 1
  --print-url` displayed `[ ] DOWNLOAD ALL EPISODES (10)` followed by Episode
  1–10. Toggling the first row and pressing Enter resolved and printed all 10
  URLs. Individual rows are independently toggleable.
- **Post-playback menu** — after a fake player exited, the fuzzy menu offered
  Replay, Download this episode instead, Next/Previous, Change quality, Pick
  another episode and Quit. Next moved E01→E02, Previous moved back, and
  Change quality switched 1080p→720p and replayed. Download this episode
  produced the normal HLS progress bar into `AP0DL_DOWNLOAD_DIR`.
- **Progress bar** — PTY download showed a live bytes/speed/ETA bar, for example
  `40.08 MiB/156.76 MiB 94.86 MiB/s`.
- **Health ordering** — writing a fresh `reanime` failure marker made
  `download 'cyberpunk 2' --print-url --range 2 --no-interactive` choose AniDB
  (`https://my.1anime.site/...`).
- **Fallback** — with AnimeParadise’s stream host pointed at
  `http://127.0.0.1:9`, `stream` reported the failure, used AniDB instead and
  exited 0.
- **Cloudflare** — `providers --probe https://anihq.cc/...` gets a 403 “Just a
  moment...” from reqwest and retries with native TLS; AniHQ still reaches a
  JavaScript/Turnstile challenge, which is reported as blocked rather than
  bypassed.
- **Legacy** — AnimePahe 403/challenge, Gogoanime 401/redirect, AllAnime
  403/`AA_CRYPTO_MISSING`, AniWave parked redirect; evidence in
  `docs/legacy-providers.md`.

## Progress log

- 2026-10-22 — Added anidb.se HTML fallback and merged episode lists; AniDB
  Cyberpunk 2 now exposes E01–E10 and resolves/downloads E02.
- 2026-10-22 — Added AnimeParadise and AnimeToki providers after live flows were
  proven; both expose 1080p and downloaded files were ffprobe-verified.
- 2026-10-22 — Extracted `interactive.rs`, `health.rs` and `fallback.rs`; stream
  and download now share pickers, progress bars and automatic provider fallback.
- 2026-10-22 — Added native `wreq` Chrome-137 TLS fallback on 403/503 challenge
  responses; verified clean failure on AniHQ’s Turnstile page.
- 2026-10-22 — Deleted the Python package, Poetry metadata, Python README and
  Python-only verification scripts; converted the harness to `jq`.
- 2026-10-22 — Rewrote README and evidence pages, added the initial checklist and
  `verify-improvements.sh`; build, clippy, 56 tests and live acceptance passed.
- 2026-10-22 — Added the download checkbox checklist with a download-all row,
  plus the post-playback fuzzy menu (replay/download/next/previous/quality).
  PTY-tested both flows; final suite now has 61 tests.

## Final verification

Commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
./scripts/verify-improvements.sh
./scripts/verify-goal.sh
```

Results:

- `cargo fmt --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo build --workspace`: pass.
- `cargo test --workspace`: **61 passed, 0 failed**.
- `verify-improvements.sh`: pass (AniDB 10 episodes, AnimeParadise + AnimeToki
  1080p, shared CLI surface, health ordering, fallback, Cloudflare probe).
- `verify-goal.sh`: full local + six-provider live acceptance; output captured in
  `docs/evidence/verify-goal-2026-10-22-rust-only.txt`.
- PTY acceptance: download checklist (download-all and independent rows) and
  stream post-playback menu (replay, download, next, previous, quality); the
  checklist and next/quit menu paths are also automated in
  `verify-improvements.sh`.
- All 21 checklist items are started/completed above; this file is mirrored to
  `/root/goals/ap0dl-rust-rewrite.md`.

## Notes

- Live acceptance is reproducible with `scripts/verify-provider.sh <provider>`,
  `scripts/verify-improvements.sh` and `scripts/verify-goal.sh`.
- Reference sources: Aniyomi extension repositories and the yuzono AniDB,
  Re:Anime, KissKH, AnimeParadise and AnimeToki behaviours used while building
  the Rust adapters.
- Provider claims are made only after a live command exits 0 on the stated date.
- Python code remains in git history, not in the working tree.