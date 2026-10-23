# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

ap0dl is a Rust-only anime search/stream/download CLI that scrapes live sites. It is a Cargo workspace with two crates:

- `crates/ap0dl-core`: all provider, HTTP, download, proxy, quality and range logic (library).
- `crates/ap0dl-cli`: the `ap0dl` binary (clap commands, dialoguer TUI pickers, provider health/fallback).

Edition 2024, MSRV 1.85. Runtime tools: `ffmpeg` (HLS remux and subtitle merge) and `mpv` (playback). The verification scripts also need `jq` and `script` (util-linux). Building `wreq` compiles BoringSSL, so `cmake` and `clang` are required.

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo test -p ap0dl-core <test_name_substring>     # single test (tests are inline #[cfg(test)] modules; there is no tests/ dir)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings   # CI-style: warnings are errors
cargo run -p ap0dl-cli -- search "anidb:Bocchi the Rock" --json
```

Live acceptance scripts hit the real sites and run `./target/debug/ap0dl`, so run `cargo build` first:

```sh
./scripts/verify-provider.sh <reanime|animegg|anidb|kisskh|animeparadise|animetoki>  # one provider: search → episodes → stream
./scripts/verify-improvements.sh   # AniDB coverage, 1080p, PTY TUI checks, health ordering, fallback, Cloudflare probe
./scripts/verify-goal.sh           # fmt + clippy + build + test + every verify-*.sh
```

`verify-checklist.sh`, which `verify-goal.sh` runs, compares `docs/goal-checklist.md` against `/root/goals/ap0dl-rust-rewrite.md`. That file is not readable on this machine, so `verify-goal.sh` fails at that step here. Run the other scripts on their own.

Use `--log debug` or `--log ap0dl_core=trace` to see tracing output. Use `--json`, `--print-url`, `--index`, `--range` and `--no-interactive` for runs without a TTY.

## Architecture

**Query resolution (`core/src/engine.rs`).** `Engine::resolve` takes three input forms:
- A URL. The provider is picked by `Provider::matches(url)` on the hostname.
- A `provider:query` prefix. Only that provider is searched.
- A bare title. Every provider is searched concurrently, each with a timeout of `timeout_secs.clamp(3, 10)`. The results are merged and ranked locally by `fuzzy_score`: token coverage plus Jaro-Winkler, and numeric tokens must match exactly, so "2" doesn't match "2099". `default_provider` only breaks ties.

`--index` is one-based and wraps around. `resolve_result` falls back to search metadata if `details` fails.

**Provider trait (`core/src/provider.rs`).** The trait is async: `search`, `details`, `episodes`, `streams`. Providers never own a client. They receive the shared `Http` (`core/src/http.rs`), which provides retries, `request_delay_ms` pacing, cookies, a TTL cache (`get_text_cached`) and the Cloudflare fallback. When `get_text` gets a 403/503 or a 200 whose body is a challenge page, it retries once through `cloudflare.rs`. That retry is a `wreq` client impersonating Chrome 137 TLS/HTTP2. It does not run JavaScript or solve Turnstile.

**Stream model (`core/src/models.rs`).** A `Stream` carries:
- `kind`: Hls, Direct or Dash
- `quality`, `audio`, `server` and `subtitles`
- per-stream `headers`, usually `Referer`, which the player and downloader must forward
- a `container` hint, for URLs without an extension such as AnimeToki's MKVs
- optional `flix: FlixInfo`

When `flix` is set, the stream is only playable through `proxy::FlixProxy`. That is a local axum server that rewrites playlists, adds headers and strips fake PNG/WebP headers and the XOR mask from segments. `download::download_stream` starts the proxy itself. `cli/commands/stream.rs::launch_stream` starts it for playback.

**Quality.** Providers expand HLS masters into one `Stream` per variant using `providers::parse_hls_variants`, so that `quality::select` can choose 1080p explicitly. The default selector is `best`. `cli/commands/mod.rs::choose_streams` returns every stream when the selector matches none.

**Health and fallback (CLI only).**
- `commands/health.rs` writes `$TMPDIR/ap0dl-provider-health/<id>` with a unix timestamp. A provider is demoted for 600 s after a stream failure, and its marker is cleared on success. The pickers sort degraded providers last.
- `commands/fallback.rs` searches the same title on other healthy providers. It accepts only an exact title match (`fuzzy_score >= 0.999`) with the same episode number, and uses an 8 s timeout per step. `stream.rs` and `download.rs` call it when a provider's streams fail or come back empty.

**TUI (`commands/interactive.rs`).** `stream` and `download` share one fuzzy anime picker. `stream` then uses a fuzzy single-episode picker, followed by a post-playback action menu (replay, download, next, previous, quality, pick another, quit). `download` uses a `MultiSelect` checklist whose first row is "DOWNLOAD ALL EPISODES". All interactive paths are skipped when stdin is not a TTY or when `--index`/`--range`/`--no-interactive` is given.

**Player (`core/src/player.rs`).** On WSL, the default `mpv` resolves to Windows `mpv.exe` when it is available. Subtitles are then passed to it as URLs, because it can't read Linux temp paths. Linux mpv under WSL gets `--vo=x11 --hwdec=no`.

**Config (`core/src/config.rs`).** Config is TOML (the old YAML format is gone). It is looked up from `AP0DL_CONFIG`, then `./ap0dl.toml`, then `~/.config/ap0dl/config.toml`, then `~/.ap0dl/config.toml`. Every provider base URL has an `AP0DL_*` env override. Tests rely on this: `verify-improvements.sh` simulates a dead upstream by setting `AP0DL_ANIMEPARADISE_STREAM=http://127.0.0.1:9`.

## Adding or changing a provider

A new provider touches several places that must stay in sync:
1. `core/src/providers/<id>.rs`: implement `Provider` and add inline parser tests.
2. `core/src/config.rs`: add a `<Name>Config` struct and a field on `ProvidersConfig`, the env overrides in `Config::apply_env`, and a `[providers.<id>]` section in `Config::sample()`.
3. `core/src/providers/mod.rs::all`: register it. Registration order is the display order.
4. Update the scripts that hard-code the provider list: `verify-cli.sh`, `verify-claims.sh`, `verify-goal.sh`, `verify-checklist.sh`, and the `case` in `verify-provider.sh`.
5. Update the README provider table. `verify-docs.sh` greps `readme.md` for required headings and provider IDs, so keep those headings intact.

Dead or blocked sites go in `core/src/legacy.rs` (shown by `ap0dl providers --legacy`) and `docs/legacy-providers.md` with the HTTP evidence observed. They are not registered.

## Project rules (from docs/goal-checklist.md and docs/legacy-providers.md)

- Register a provider, or mark it `Stable`, only after a live search → details → episodes → stream smoke test passes. Record the date and the evidence under `docs/evidence/`.
- No Selenium, Playwright or browser automation, and no attempts to get past JavaScript or Turnstile challenges. If a site needs them, document it as blocked.
- The tree is Rust-only. `verify-goal.sh` fails if any `*.py` file exists. The old Python implementation is only in git history.

## Manga plugin (`crates/ap0dl-manga`)

Optional crate mounted in `ap0dl-cli` behind the `manga` feature (off by default; see `docs/manga.md`). Build, lint and test it with `--features ap0dl-cli/manga` / `--all-features`; `cargo clippy --workspace --all-targets --all-features -- -D warnings` must pass. It depends on `ap0dl-core` but core never depends on it.

- `source::MangaSource` is the Aniyomi-style trait; register sources only in `sources::all`, add a config struct in `config.rs` (`[manga.sources.<id>]`, env overrides in `apply_env`, sample entry), and add the source to `scripts/verify-manga.sh`.
- `cli/` holds the whole `ap0dl manga` surface; the host only has the `Manga(MangaArgs)` variant and one dispatch arm in `main.rs`.
- `Page::headers` must carry any CDN `Referer`; `Page::fallback_urls` lists mirrors tried after `image_url` fails.
- Live check: `./scripts/verify-manga.sh <source>`.
