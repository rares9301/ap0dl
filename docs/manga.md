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
ap0dl manga read ["title"] [--range R] [--reader mpv] [--print-url]
ap0dl manga download ["title"] [--range R] [-d dir] [--format cbz|folder]
ap0dl manga library list|add|remove|update [--download]
ap0dl manga config [--sample]
```

`read` and `download` share the anime UX: a fuzzy title picker, a fuzzy
chapter picker (read) or a checkbox list with `DOWNLOAD ALL CHAPTERS`
(download), and a post-chapter menu (next, previous, read again, download,
add to library, pick another, quit). `--index`, `--range`, `--no-interactive`,
`--print-url` and `--json` keep everything scriptable. Ranges use the episode
syntax plus `latest` / `latest-N`; duplicate uploads of one chapter collapse
to one, preferring `--scanlator` when given.

mpv-family readers receive page URLs directly (with the source's `Referer`);
other viewers get a temporary folder of downloaded pages.

## Configuration

The plugin reads a `[manga]` table from the same config file as the anime
settings. Print a sample with `ap0dl manga config --sample`. Environment
overrides: `AP0DL_MANGA_DEFAULT_SOURCE`, `AP0DL_MANGA_DIR`,
`AP0DL_MANGA_FORMAT`, `AP0DL_MANGA_READER`, `AP0DL_MANGA_LIBRARY`,
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
cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/verify-manga.sh mangadex     # one source, live
./scripts/verify-manga.sh              # all sources + TUI, library, fallback
```

Live result for this branch is in `docs/evidence/manga-mangadex.txt`.
