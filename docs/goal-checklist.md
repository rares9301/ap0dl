# Goal: AP0DL — production-ready Android app polish

Status: in progress — started 2026-10-23
Created: 2026-10-23
Owner: user
Project: /root/ap0dl
Previous goal archive: [`docs/goal-checklist-rust-2026-10-22.md`](goal-checklist-rust-2026-10-22.md)

## Objective (final goal)

Ship the AP0DL Android app as a production-ready experience: the display name is
**AP0DL**, the dashboard is minimalist and free of filler copy, posters and metadata
respect their rounded cards, continue-watching shows a circular progress ring and
minutes remaining without marking an episode complete after one second, the player
behaves like Aniyomi/Netflix (horizontal seek, left brightness / right volume
vertical gestures, double-tap seek, clean auto-hiding controls and skip-intro/outro
with an AniSkip backend and a `+85s` fallback), the app adds a live **AniTrendz
trending** tab and an original **Schedule** tab, credits the real team, shows a
playful **294M tokens** counter, and is verified on an emulator with screenshots
before the code is committed and pushed.

## Authorization / constraints

- Owner authorized the app work, emulator installation, screenshots and live testing.
- Rust owns providers, extraction, quality, schedules, trending and AniSkip logic.
  Kotlin remains UI/player glue only.
- Do not remove dubs. Sub/Dub stays selectable and the 1080p preference stays intact.
- Do not claim a provider works unless a live smoke test succeeded.
- The existing Android package id (`com.ap0dl.anyomi`) stays stable so upgrades do
  not create a second installed app; user-facing branding becomes AP0DL.
- Do not commit test media (`Cyberpunk - Edgerunners 2/`, `OSHI NO KO Season 3/`).

## Definition of done

`cargo test --workspace` and the Android release build pass; the release APK is
installed and exercised on a fresh emulator; Home, Browse, Details, Trending,
Schedule, Downloads, More and the new player are screenshot-verified; a live
stream plays with the gesture overlay and skip-intro query; continue-watching
round-trips a real position and shows minutes-left/progress; AniTrendz and the
schedule load live data; the checklist below is fully marked; code is committed
and pushed to `origin/main`; the session watchdog is disarmed with a completion
marker.

## Checklist (ordered)

Status legend: `[ ]` not started · `[~]` started / in progress · `[x]` completed.

| # | Work item | Started | Completed | Evidence / notes |
|---|-----------|---------|-----------|------------------|
| 1 | Recon and UX audit: live AniTrendz shape, AniList schedule shape, Aniyomi player behaviour, current screenshots | [x] | [x] | AniTrendz `__NEXT_DATA__` top-anime chart has rank/name/images (64 entries, Oct 18 2026 week); AniList GraphQL airing schedule returned 50+ entries for the current week; Aniyomi gestures researched; emulator booted |
| 2 | Goal contract and crash watchdog: ordered checklist, heartbeat, exact-session recovery script | [x] | [x] | This file + `scripts/ap0dl-watchdog.sh`; watchdog armed as PID 320372 for `ses_ea46ffdedffe0RCxH722avtESK`; host Goal V2 lifecycle is read-only from the model surface, so the app goal is tracked here |
| 3 | Rebrand and copy pass: AP0DL display name, strings, remove greeting/“Search once”/“obsession”/“without the noise”/“resolved live by the rust engine”; keep “made with a Rust heart” | [x] | [x] | `strings.xml`, Home, Browse, More; version tag replaces slogan; provider summaries are viewer-facing; footer kept |
| 4 | Credits and token counter: Rares-Andrei Sarmasag (`rares9301`), Victor Iusco (`Victor-Iusco`), DeepSeek V4.1 Flash, Claude Opus 5.5, 294M tokens | [x] | [x] | More → Team card and “294M tokens incinerated” card; screenshot `more-crest.png` |
| 5 | Dashboard and cards: minimalist Home, poster/year clipping fix, rounded surfaces, clean section headings | [x] | [x] | Column clip removed; only artwork is clipped; “2026 · TV” fully visible; Home screenshot |
| 6 | Continue watching correctness: duration-aware entries, <95% completion threshold, circular progress ring, “X min left”, no instant completion | [x] | [x] | Live round-trip showed 12 min left, ring, and 6 min left for second title; position persisted across restart |
| 7 | Rust media extras: AniSkip JNI endpoint with fallback metadata, AniTrendz parser, AniList weekly schedule endpoint | [x] | [x] | `anitrendz.rs` (64 live entries), `schedule.rs` (130 live episodes), `nativeSkipTimes` JNI; fmt/clippy/tests pass |
| 8 | Player overhaul: custom controller over Media3, horizontal seek, left brightness / right volume, double-tap seek, HUD, auto-hide, clean controls and audio/quality sheets | [x] | [x] | Aniyomi 0.15 s/px seek, 2/5–3/5 double-tap zones, side HUDs, center-tap guard, custom subtitle style; gesture screenshots |
| 9 | Skip intro/outro: real AniSkip ranges when a MAL id exists, Aniyomi-style `+85s` fallback otherwise | [x] | [x] | Re:Anime jumped to 3:28 via AniSkip; AnimeGG (no MAL id) showed the `Skip intro` fallback and skipped live |
| 10 | AniTrendz trending tab: live rank list, covers, movement metadata, refresh, tap-to-search | [x] | [x] | Live 64 titles, week 2 fall, movement chips; tapping rank 1 opened Kusuriya no Hitorigoto details |
| 11 | Schedule tab: original weekly timeline UI from AniList airing data, day selector, local times, refresh | [x] | [x] | Live 130 episodes, day counts, local times, countdowns, error/empty states |
| 12 | Navigation: five-tab production bottom bar (Home, Trending, Schedule, Downloads, More) with Browse reachable from search/source cards | [x] | [x] | Home search opens Browse; Browse remains a route without its own tab |
| 13 | Build gates: `cargo fmt --check`, clippy, `cargo test --workspace`, Android release build | [x] | [x] | `cargo fmt --check` clean; clippy `-D warnings` clean; 67 tests pass; R8 release build success |
| 14 | Emulator verification: fresh boot, install release APK, live navigation/player/gesture/skip/progress tests, screenshots | [x] | [x] | Home, Trending, Schedule, Downloads, More, Browse, player HUDs, AniSkip and +85 fallback verified; screenshots in `docs/evidence/ap0dl-2026-10-23/` |
| 15 | Custom AP0DL identity art: launcher icon + About artwork memeing UPB Politehnica Bucharest, DeepSeek, Arch Linux and Rust | [x] | [x] | Adaptive crest (Arch A, Rust gear, DeepSeek wave, UPB shield, play mark), splash reuse, More “AP0DL crest” card, launcher screenshot |
| 16 | Netflix-beating polish pass: interaction details, empty/error states, provider labels, player ergonomics, no regressions | [x] | [x] | Fixed Re:Anime label, Browse/Downloads copy, duplicate Downloads headline, center-tap pause, horizontal seek units; player, trending, schedule and history flows re-tested |
| 17 | Docs and release: README, version/APK, checklist final state, commit, push to `origin/main` | [~] | [ ] | README/evidence in progress; APK 1.1.0 built; commit/push pending |
| 18 | Finish: mark goal complete, disarm watchdog, report outcome | [~] | [ ] | Waiting for final push + evidence + marker |

## Recovery contract

If the session is interrupted, the watchdog resumes the same session id with a
prompt pointing at this file. The recovery agent must:

1. read this file and the latest `git status`/`git diff`;
2. continue from the first `[~]` or `[ ]` item, never redo a completed item;
3. update started/completed columns as work advances;
4. keep test media out of commits;
5. finish the build/emulator verification, commit and push;
6. write the completion marker only after the push and final evidence exist.