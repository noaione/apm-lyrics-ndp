# Apple Music Lyrics Plugin for Navidrome

[![Build](https://github.com/noaione/apm-lyrics-ndp/actions/workflows/build.yml/badge.svg)](https://github.com/noaione/apm-lyrics-ndp/actions/workflows/build.yml)

A Navidrome plugin that fetches synced (syllable/TTML) lyrics from Apple Music for tracks that are already tagged with an Apple Music/iTunes catalog ID.

> [!IMPORTANT]
> This plugin requires an active Apple Music subscription, and only works for tracks that already carry an iTunes Catalog ID tag (e.g. files purchased from, or matched against, the iTunes/Apple Music catalog). It does not search by title/artist.<br />
> **If you want search-based lyrics, consider using [navidrome-lyrics-plugin](https://github.com/J0R6IT0/navidrome-lyrics-plugin) instead.**

## How it works

1. Reads the iTunes Catalog ID from the track's tags (`ITUNESCATALOGID` for Vorbis comments, `iTunes Catalog ID` user text frame for ID3v2/APE/RIFF, or the `cnID` atom for MP4).
2. Reuses a cached JWT (or scrapes a fresh one from `beta.music.apple.com`) to authenticate against Apple's private `amp-api.music.apple.com` API.
3. Fetches syllable lyrics (TTML) for the catalog ID and returns them to Navidrome.
4. Caches the resulting lyrics for a configurable number of days.

## Requirements

- Navidrome with plugin support enabled. (Recommended running on v0.63.0+ for syllable lyrics support)
- An active Apple Music subscription.
- Tracks tagged with their Apple Music/iTunes catalog ID.

## Installation

1. Download the latest `apm-lyrics-ndp.ndp` from the [releases page](https://github.com/noaione/apm-lyrics-ndp/releases) and place it in your Navidrome plugins folder (default: `<navidrome-data-directory>/plugins/`).
2. Add `apm-lyrics-ndp` to the `LyricsPriority` [configuration option](https://www.navidrome.org/docs/usage/configuration/options/#:~:text=true-,LyricsPriority,-ND_LYRICSPRIORITY), e.g.:
   ```toml
   # navidrome.toml
   LyricsPriority = ".ttml,apm-lyrics-ndp,embedded,.lrc"
   ```
   Or via an environment variable:
   ```
   ND_LYRICSPRIORITY=.ttml,apm-lyrics-ndp,embedded,.lrc
   ```
3. Restart Navidrome, then enable and configure the plugin under **Settings > Plugins**.

## Configuration

| Field                  | Description                                                                                                                                                           |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `media_token`          | Your Apple Music `media-user-token`, required to call the private API.                                                                                                |
| `user_agent`           | The `User-Agent` used when logging in. Must match the token's origin session.                                                                                         |
| `storefront`           | Your Apple Music account's storefront country code (e.g. `us`, `gb`, `jp`).                                                                                           |
| `cache_days`           | How many days to cache fetched lyrics for (1-30, default 7).                                                                                                          |
| `skip_cache`           | Always re-fetch from Apple Music instead of using the cache. Useful for debugging/updating lyrics.                                                                    |
| `translation_language` | The translation language, should be something that is allowed for your account (You can check by playing song then opening lyrics in the Apple Music Beta), optional. |

### Getting `media_token` and `user_agent`

1. Log into [beta.music.apple.com](https://beta.music.apple.com) in a browser, with an account that has an active Apple Music subscription.
2. Play a music track that has lyrics available (e.g. a song from the Apple Music catalog).
3. Open your browser's developer tools and inspect a request made to `amp-api.music.apple.com`.
4. Copy the `media-user-token` request header value into `media_token`.
5. Copy that same request's `User-Agent` header into `user_agent` — Apple ties the session to the exact user agent it was issued with, so it must match.
6. Set `storefront` to the storefront your account is registered to.

## Permissions

This plugin declares the following permissions in `manifest.json`:

- **http** — to `amp-api.music.apple.com`, `beta.music.apple.com`, and `music.apple.com`, to authenticate and fetch lyrics.
- **library** (raw filesystem access) — to read the iTunes Catalog ID tag directly from track files.
- **kvstore** — to cache fetched lyrics.
- **cache** — to cache the scraped JWT between requests.

## Building

### Prerequisites

- [Rust](https://rustup.rs/) with the `wasm32-wasip1` target: `rustup target add wasm32-wasip1`
- `make` and `zip`

### Build

```sh
make build
```

This compiles the plugin and produces `apm-lyrics-ndp.ndp`, ready to drop into your Navidrome plugins folder.
