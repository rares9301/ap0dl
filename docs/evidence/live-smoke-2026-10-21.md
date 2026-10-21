# Live smoke evidence — 2026-10-21

Captured by the Rust `ap0dl` binary from this workspace. Raw command output is
under `docs/evidence/raw/`; this page summarizes the same files. A provider is
listed as working only because the commands below exited 0 and returned the
shown data. Stream URLs, request headers and signed tokens are redacted in the
committed raw evidence.

## Re:Anime

```sh
ap0dl search "reanime:Bocchi the Rock" --json --limit 3
ap0dl episodes "reanime:Bocchi the Rock" --json
ap0dl grab "reanime:Bocchi the Rock" --range 1 --quality best
```

- Search returned `BOCCHI THE ROCK!, Bocchi the Rock! 2nd Season, BOCCHI THE ROCK! Recap Part 1`.
- Details/episodes: **Bocchi the Rock!**, **12 episodes**.
- Streams: `1080p` hls [Sub] HD-1, `1080p` hls [Sub] HD-2.

## AnimeGG

```sh
ap0dl search "animegg:Frieren" --json --limit 3
ap0dl episodes "animegg:Frieren" --json
ap0dl grab "animegg:Frieren" --range 1 --quality best
```

- Search returned `Frieren beyond journeys end season 2, Sousou no Frieren, Sousou no Frieren: ●● no Mahou`.
- Details/episodes: **Frieren beyond journeys end season 2**, **10 episodes**.
- Stream: `1080p` direct [Dub] AnimeGG 1080p.

## AniDB / Anila

```sh
ap0dl search "anidb:Bocchi the Rock" --json --limit 3
ap0dl episodes "anidb:Bocchi the Rock" --json
ap0dl grab "anidb:Bocchi the Rock" --range 1 --quality 1080
```

- Search returned `Bocchi the Rock!`.
- Details/episodes: **Bocchi the Rock!**, **12 episodes**.
- Stream: `1080p` hls [Sub] SUB Server #1 1080p.

## KissKH

```sh
ap0dl search "kisskh:Bocchi the Rock" --json --limit 3
ap0dl episodes "kisskh:Bocchi the Rock" --json
ap0dl grab "kisskh:Bocchi the Rock" --range 1 --quality best
```

- Search returned `Bocchi the Rock!, Camp Rock, Rock It, Mom`.
- Details/episodes: **Bocchi the Rock!**, **12 episodes**.
- Stream: `720p` hls KissKH 720p.

KissKH served 720p for this and the other sampled titles; no 1080p variant was
observed, so it is not labeled as 1080p.

## Legacy providers

```sh
ap0dl providers --legacy --json
```

- **AnimePahe** (`animepahe`): deprecated; observed 2026-10-21. GET https://animepahe.pw/ -> HTTP 403, 5,102 bytes, title "Just a moment..." (Cloudflare). GET https://animepahe.su/ -> HTTP 200, 6,990-byte challenge shell with no usable API JSON.
- **Gogoanime / Anitaku** (`gogoanime`): deprecated; observed 2026-10-21. GET https://gogoanime.bid/ -> redirect to http://ww547.gogoanime.bid/?tkn=... then HTTP 401, 9 bytes. GET https://anitaku.pe/search.html?keyword=Bocchi -> HTTP 200 but a 27,666-byte parked "abovedomains" for-sale page, not results.
- **AllAnime** (`allanime`): deprecated; observed 2026-10-21. GET https://api.allanime.day/api with a shows search query -> HTTP 403, 7,231-byte Cloudflare "Just a moment..." page. The previous source query had already returned AA_CRYPTO_MISSING.
- **AniWave / 9anime** (`aniwave`): deprecated; observed 2026-10-21. GET https://aniwave.to/ -> HTTP 200 after redirect to https://ww38.aniwave.to/, 44,059 bytes, title "aniwave.to", no watch/search links (parked domain).

## Reproduction

Run the commands above from `/root/ap0dl` after `cargo build --workspace`.
Network availability and CDN behavior can change; a failed live command invalidates
the corresponding working claim until it is re-run successfully.
