# Manga plugin (`ap0dl-manga`)

Manga support lives in its own crate, `crates/ap0dl-manga`, and is mounted
into the `ap0dl` binary behind the `manga` cargo feature. The feature is
**off by default** so the plugin can be reviewed before it ships.

```sh
cargo build --release --features ap0dl-cli/manga
./target/release/ap0dl manga --help
```

To ship it, add `manga` to `default` in `crates/ap0dl-cli/Cargo.toml`. To
remove it, drop the feature, the optional dependency and the two
`#[cfg(feature = "manga")]` lines in `crates/ap0dl-cli/src/main.rs`. The core
crate is not modified by the plugin.

## Design (Aniyomi model)

| Aniyomi | ap0dl-manga |
|---|---|
| `SManga`, `SChapter`, `Page`, `MangasPage` | `models::{Manga, Chapter, Page, MangasPage}` |
| `CatalogueSource` / `HttpSource` | `source::MangaSource` (`popular`, `latest`, `search` with filters, `details`, `chapters`, `pages`, `image_url`, `image_headers`) |
| `getFilterList()` | `filters::{Filter, FilterValues}`, validated before a source sees them |
| Library + library update | `library::Library` (JSON), new-chapter detection, read progress |
| Downloader | `download` + `cbz`: CBZ with `ComicInfo.xml`, or image folders |
| Migrate | `MangaEngine::fallback_pages`: same title and chapter on another source |
| Reader (paged, dual page, continuous) | `viewer` + `spread`: mpv over its IPC socket, spreads joined by ffmpeg |
| Reading history | `state::ReadingState`: last chapter and page per series |

Like anime providers, sources receive the shared `Http` client, so retries,
pacing, cookies and the Cloudflare fallback are uniform. Search runs on every
source concurrently and is ranked locally with the anime fuzzy scorer
(alternative titles count, at a slight discount).

## Commands

```sh
ap0dl manga sources [--filters <source>]
ap0dl manga search "title" [--source id --filter key=value ...]
ap0dl manga popular|latest --source <id> [--page N]
ap0dl manga chapters "source:title" [--json]
ap0dl manga pages "source:title" --range 1
ap0dl manga read ["title"] [--range R] [--reader mpv] [--single|--double] [--rtl|--ltr] [--print-url]
ap0dl manga download ["title"] [--range R] [-d dir] [--format cbz|folder]
ap0dl manga library list|add|remove|update [--download]
ap0dl manga config [--sample]
```

`read` and `download` share the anime UX: a fuzzy title picker, then either
the chapter prompt (read) or a checkbox list with `DOWNLOAD ALL CHAPTERS`
(download). `--index`, `--range`, `--no-interactive`, `--print-url` and
`--json` keep everything scriptable. Ranges use the episode syntax plus
`latest` / `latest-N`; duplicate uploads of one chapter collapse to one,
preferring `--scanlator` when given.

### Reading

`read` asks **What chapter do you want to read?** Pressing Enter with nothing
typed takes the default in brackets: the saved chapter, which reopens at the
saved page. With no saved state, the default is the library's next unread
chapter, or else the first chapter. Other answers:

- a number such as `12` or `ch 10.5` (if it doesn't exist, the closest chapter opens)
- `first` or `latest`
- part of a chapter title
- `list`, which opens the fuzzy picker

With `mpv` the chapter opens in a **continuous viewer**: one mpv window that
runs on from chapter to chapter. While you read, the next chapter is
downloaded into a temporary cache and appended to the playlist. Stepping back
past the first page loads the previous chapter. Chapters more than two behind
are dropped from the cache.

| Key | Action |
|---|---|
| Left / Right | next / previous page in `rtl` (swapped in `ltr`) |
| Space, Down, PgDn, Enter, j, wheel down | next page |
| Backspace, Up, PgUp, k, wheel up | previous page |
| n / p | next / previous chapter |
| d | single / double page |
| o | shift the pairs by one page (shows the first page alone) |
| f | fullscreen |
| ? | key help |
| q | quit |

Double page layout pairs portrait pages into one spread, with the first page
on the right in `rtl`. ffmpeg joins each pair. Landscape pages (already
spreads), webtoon strips and pages of unknown size stay single, so a 2-page
chapter is one spread. Without ffmpeg the viewer shows single pages.

After mpv closes, a menu offers:

- continue reading
- read another chapter
- download this chapter
- add to library
- quit (the default)

Reading state is saved on every page turn to `manga-state.json` in the
config directory (`state_path`, `AP0DL_MANGA_STATE`). For each series it
records:

- chapter
- page
- the layout last used, which takes precedence over `page_layout` for that series

A saved title also matches the same series on another source, by chapter
number. Followed series are marked read when their last page is shown.

The viewer needs a Unix mpv, because it talks to mpv over a Unix socket. On
WSL, when the default reader resolves to Windows `mpv.exe`, Linux `mpv` is
used if it is installed. If it isn't, or the reader is another program, or
`--print-url` is given, `read` keeps the per-chapter flow:

- mpv-family readers get page URLs with the source's `Referer`
- other viewers get a temporary folder of downloaded pages
- a next / previous / reread / download / follow / pick / quit menu follows each chapter

## Configuration

The plugin reads a `[manga]` table from the same config file as the anime
settings. Print a sample with `ap0dl manga config --sample`. Environment
overrides: `AP0DL_MANGA_DEFAULT_SOURCE`, `AP0DL_MANGA_DIR`,
`AP0DL_MANGA_FORMAT`, `AP0DL_MANGA_READER`, `AP0DL_MANGA_LIBRARY`,
`AP0DL_MANGA_STATE`, `AP0DL_MANGA_LAYOUT` (`single`/`double`),
`AP0DL_MANGA_DIRECTION` (`rtl`/`ltr`),
`AP0DL_MANGADEX_BASE`, `AP0DL_MANGADEX_API`, `AP0DL_MANGADEX_UPLOADS`,
`AP0DL_MANGADEX_LANGUAGES`, `AP0DL_MANGAPILL_BASE`, `AP0DL_WEEBCENTRAL_BASE`.

## Sources

| ID | Notes |
|---|---|
| `mangadex` | Public JSON API. Page requests are paced for the at-home limit (40/min); pages fall back from the @Home node to the origin server. Licensed series often have no English chapters. |
| `mangapill` | HTML catalogue; images need the site `Referer`. |
| `weebcentral` | htmx fragments for search, chapter list and reader pages. |

## Verification

```sh
cargo test -p ap0dl-manga
cargo test -p ap0dl-manga mpv_session -- --ignored   # real mpv + ffmpeg viewer session, offline
cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/verify-manga.sh mangadex     # one source, live
./scripts/verify-manga.sh              # all sources + TUI, chapter prompt, viewer, library, fallback
```

Live result for this branch is in `docs/evidence/manga-mangadex.txt`.
