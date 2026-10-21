# Goal: ap0dl — Rust engine + live providers + legacy audit

Status: complete — implementation and verification finished 2026-10-21
Created: 2026-10-21
Owner: user
Project: /root/ap0dl
OpenCode Goal ID: 043fb3b9-5b4e-4842-832f-dd1fe3fd9412 (session ses_ea46ffdedffe0RCxH722avtESK)
Goal state: /root/ap0dl/.opencode/goals/fa951d918e95d72c85547a279f45d93c.json
Goal revision: 3

## Objective

Replace the Python ap0dl engine with a typed, async Rust engine, then repair or
replace the anime-site providers using the Aniyomi extension sources as
behavioural references. Expand the live provider set beyond Re:Anime and
AnimeGG, including KissKH and AniDB/Anila, prefer 1080p when the source offers
it, and explicitly document legacy providers that are dead/blocked.
Re:Anime remains the primary, best-supported source.

## Authorization / constraints

- Owner authorized broad sandbox experimentation and asked to remove the
  site-terms, read-only-scraping and rate-limit constraints from the goal.
  The goal contract therefore contains only the two evidence/integrity
  constraints below.
- Implementation still uses ordinary GET requests and a small request delay for
  CDN reliability; that is an engineering choice, not a goal constraint.
- Do not claim a provider is working unless a live smoke test succeeded.
- Do not delete the existing Python code before the Rust replacement is verified.
- Legacy providers must be live-repaired or explicitly deprecated with observed
  HTTP evidence.

## Definition of done

A Rust workspace builds with no warnings, has a single `ap0dl` binary exposing
`search`, `episodes`, `grab`, `stream`, `download`, `providers` and `config`;
every registered provider can be exercised against its live site; 1080p is
exposed when upstream offers it and is the default preference; legacy providers
are documented with evidence; the Python package remains for migration/reference.
The authoritative checklist is this file and is updated at each milestone.

## Checklist (ordered)

Status legend: `[ ]` not started · `[~]` in progress (Started `[x]`) · `[x]` done.

| # | Work item | Started | Completed | Notes |
|---|-----------|---------|-----------|-------|
| 1 | Recon: repo audit, live-site probes, extension-source research | [x] | [x] | Python layout mapped; Re:Anime API + Flixcloud flow proven; yuzono sources used as reference |
| 2 | Rust workspace skeleton (`crates/ap0dl-core`, `crates/ap0dl-cli`) | [x] | [x] | Cargo workspace, 2024 edition, shared dependencies, error type, module layout |
| 3 | Core engine: config, models, async HTTP, cache, rate-limit, provider trait/registry, episode-range + quality parsers | [x] | [x] | Typed concepts replace loose Python dicts/partials; `cargo build --workspace` clean |
| 4 | Re:Anime provider: search, details, episodes, Flixcloud stream resolution | [x] | [x] | Live: search/details/12 episodes; Bocchi E1 HD-1/HD-2 1080p HLS; local decrypt proxy |
| 5 | AnimeGG provider: search, details, episodes, direct MP4 resolution | [x] | [x] | Live: search/details/episodes; Frieren S2 E1 1080p MP4; Bocchi E1 only 720p/480p upstream |
| 6 | AniDB / Anila provider: search, details, episodes, server iframes, HLS variants | [x] | [x] | Live: 12 episodes; Bocchi E1 1080p/720p/360p HLS; 360p download verified playable |
| 7 | KissKH provider: search, details, episodes, Apps Script episode key, CDN HLS | [x] | [x] | Live: 12 episodes; Bocchi/Frieren/Solo Leveling 720p HLS observed |
| 8 | 1080p policy: expose variants and make default quality prefer 1080p | [x] | [x] | Default `quality = "best"`; AniDB/ReAnime/AnimeGG expose 1080p; selector test added |
| 9 | AnimePahe audit and disposition | [x] | [x] | Deprecated: 403 Cloudflare on `.pw`; 200 challenge shell on `.su`; no API/stream path |
| 10 | Gogoanime/Anitaku audit and disposition | [x] | [x] | Deprecated: `.bid` redirects to HTTP 401 token endpoint; `anitaku.pe` is parked |
| 11 | AllAnime audit and disposition | [x] | [x] | Deprecated: API Cloudflare 403; earlier source query `AA_CRYPTO_MISSING` |
| 12 | AniWave/9anime audit and disposition | [x] | [x] | Deprecated: redirects to parked `ww38.aniwave.to`; no watch/search links |
| 13 | Download engine: direct MP4 + HLS parser, AES-128, image-header stripping, XOR segments, resume, ffmpeg merge | [x] | [x] | Re:Anime AES HLS, AnimeGG direct MP4 and AniDB header-authenticated HLS downloaded |
| 14 | CLI: search, episodes, grab, stream, download, providers, config; provider prefixes/URL detection; range/quality flags | [x] | [x] | `providers --legacy` exposes deprecation evidence; no empty-result panic |
| 15 | Tests: quality/ranges/HLS/AES/proxy/provider parsers + legacy evidence | [x] | [x] | `cargo test --workspace`: 42 passed, 0 failed (re-run in final verification) |
| 16 | Docs: Rust README, provider matrix, config/migration, legacy evidence page | [x] | [x] | `readme.md` rewritten; `docs/legacy-providers.md`; old README preserved as `docs/python-readme.md` |
| 17 | Python retention / migration boundary | [x] | [x] | Python code retained untouched; Rust engine is the `ap0dl` binary; config format migration documented |
| 18 | Final verification and requirement-by-requirement audit | [x] | [x] | `cargo build --workspace`, `cargo test --workspace` (42 passed), `cargo fmt --check`, `cargo clippy -D warnings` all pass; final live smokes for all four providers passed |

## Live smoke evidence (2026-10-21)

All commands used the built Rust binary from this workspace.

- **Re:Anime** — `ap0dl grab 'reanime:Bocchi the Rock' --range 1 --quality best`
  returned `[Sub] HD-1` and `[Sub] HD-2`, both `1080`/HLS.
- **AnimeGG** — `ap0dl grab 'animegg:Frieren' --range 1 --quality best` returned
  a `1080` direct MP4. `animegg:Bocchi the Rock` E1 returned 720p and 480p; the
  live embed's `videoSources` JSON contains only those two labels, so this is an
  upstream limitation, not dropped extraction.
- **AniDB / Anila** — `ap0dl search 'anidb:Bocchi the Rock'`,
  `ap0dl episodes 'anidb:Bocchi the Rock' --json`, and
  `ap0dl grab ... --quality 1080` returned 12 episodes and a 1080p HLS stream.
  `ap0dl download ... --quality 360 --no-merge` produced
  `/tmp/ap0dl-anidb/Bocchi the Rock!/E01.ts` (45,170,008 bytes, MPEG-TS sync
  every 188 bytes).
- **KissKH** — search, details, 12 episodes and
  `ap0dl grab 'kisskh:Bocchi the Rock' --quality best` returned a 720p HLS
  stream. Frieren and Solo Leveling also returned 720p. No 1080p variant was
  observed for these titles.
- **Stream URL smoke** — `ap0dl stream ... --print-url` printed a 1080p AniDB
  master variant and a KissKH HLS URL.
- **Legacy** — `ap0dl providers --legacy` prints the observations recorded in
  `docs/legacy-providers.md`: AnimePahe 403/challenge, Gogoanime 401/redirect,
  AllAnime 403/`AA_CRYPTO_MISSING`, AniWave parked redirect.
- **Regression fixed** — an empty explicit-provider search previously caused a
  divide-by-zero panic in `Engine::resolve`; it now returns `NoResults` cleanly.

## Progress log

- 2026-10-21 01:xx — Repo audited. Rust 1.98.1 available. Re:Anime API verified
  end-to-end via `/api/v1/search` → details → episodes → `/api/flix/...` →
  Flixcloud → enc-dec proxy master.
- 2026-10-21 02:xx — Core engine, CLI, Re:Anime and AnimeGG implemented and live
  verified. HLS/AES and direct MP4 downloads verified.
- 2026-10-21 03:xx — Goal constraints updated per owner request: removed
  site-terms/read-only/rate-limit constraints; added broader-source and 1080p
  requirements. Added AniDB/Anila and KissKH providers; verified live search,
  details, episodes and streams. Added legacy CLI evidence and docs; rewrote
  README with provider matrix/migration; fixed empty-search panic; rebuilt and
  ran 42 tests.
- 2026-10-21 06:3x — Per-provider assertion scripts (`scripts/verify-provider.sh`) re-ran all four live source flows successfully.
- 2026-10-21 06:4x — Final audit passed. Re-ran all four live provider smokes
  after the last edits: Re:Anime 1080p HLS, AnimeGG 1080p direct MP4, AniDB
  1080p HLS + 12 episodes, KissKH 720p HLS + 12 episodes. `cargo build
  --workspace`, `cargo test --workspace` (42/42), `cargo fmt --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` all pass. The Rust
  engine replaces the Python engine for the CLI; Python remains untouched.

## Notes

- The authoritative checklist is `docs/goal-checklist.md`; a copy is maintained at
  `/root/goals/ap0dl-rust-rewrite.md`.
- Live acceptance is reproducible via `scripts/verify-provider.sh <provider>` and
  the full harness `scripts/verify-goal.sh`. Provider claims are made only after
  those commands pass against the live sites.
- Completion is claimed only after build/test, all four live provider checks and
  the legacy evidence checks pass.
- Reference sources:
  - https://github.com/Secozzi/aniyomi-extensions
  - https://github.com/yuzono/anime-extensions (ReAnime, AniDB, KissKH and
    related EN scrapers)
  - yuzono ReAnime Kotlin source for the API + Flixcloud flow
  - yuzono KissKH Kotlin source for the Apps Script key flow
