# Legacy provider audit — 2026-10-21

The four providers below were part of the Python ap0dl lineage. Each was
probed from this sandbox before the Rust engine declared its provider set. None
had a usable live search → details → episodes → stream path, so none is
registered in the Rust engine. The Rust `providers` command reports them as
deprecated and prints the observed evidence:

```sh
ap0dl providers --legacy
```

The test user agent was a normal desktop Chrome string. Responses were fetched
with redirects enabled. Byte counts are the body sizes observed on this date.

## AnimePahe

- `GET https://animepahe.pw/` → **HTTP 403**, 5,102 bytes, title
  `Just a moment...` (Cloudflare challenge).
- `GET https://animepahe.su/` → **HTTP 200**, 6,990 bytes, a challenge shell with
  no usable search/API JSON.
- No episode or stream extraction was possible.

## Gogoanime / Anitaku

- `GET https://gogoanime.bid/` → redirect to
  `http://ww547.gogoanime.bid/?tkn=...` and then **HTTP 401**, 9-byte response.
- `GET https://anitaku.pe/search.html?keyword=Bocchi` → **HTTP 200**, 27,666
  bytes, but the body is an `abovedomains.com` parked/for-sale page, not search
  results.
- No usable search result HTML or episode pages were available.

## AllAnime

- A shows search GraphQL request to `https://api.allanime.day/api` returned
  **HTTP 403**, 7,231 bytes, title `Just a moment...` (Cloudflare challenge).
- An earlier episode source query against the same API returned
  `{"errors":[{"message":"AA_CRYPTO_MISSING"...}]}`; the current crypto/token
  scheme is not implemented.
- Search metadata alone is not sufficient for a working provider, so the
  adapter remains deprecated.

## AniWave / 9anime

- `GET https://aniwave.to/` → **HTTP 200** after redirect to
  `https://ww38.aniwave.to/`, 44,059 bytes, title `aniwave.to`, with no watch or
  search links. The destination is a parked domain.
- No episode/server extraction was possible.

## Repair criteria

A legacy entry may be moved into the registered provider list only after all of
the following succeed against the live site from the current workspace:

1. search returns the intended series;
2. details resolve that series;
3. episodes list the expected range;
4. streams resolve to a playable URL;
5. the Rust CLI completes `grab` without an error;
6. the provider is documented with the smoke-test date.

Until then, the source remains deprecated with the evidence above.

## Historical Python README

The old README is preserved at [`python-readme.md`](python-readme.md) for
migration/reference. The Python package itself remains in the repository.
