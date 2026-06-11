> [!NOTE]
> This project is developed with the assistance of AI code generation tools. AI-generated code is reviewed and tested before being merged, but if you encounter any issues, please feel free to open an issue or submit a pull request.

> [!WARNING]
> This project is in active development. Expect breaking changes to the database schema and cache key format until we reach a stable release. You may need to delete your database or cache after updating in order to not retain orphaned cache items.

# OpenPosterDB (OPDB)

A self-hosted, drop-in replacement for [RPDB (Rating Poster Database)](https://ratingposterdb.com). Generates movie and TV show posters, logos, and backdrops with rating badges from multiple sources overlaid on them. Fetches art from TMDB (or optionally [Fanart.tv](https://fanart.tv)), aggregates ratings from IMDb, Rotten Tomatoes, Metacritic, Trakt, Letterboxd, MyAnimeList, the MDBList score, and Roger Ebert, and composites color-coded badges onto the image.

[GitHub](https://github.com/pnrxa/openposterdb) | [Docker Hub](https://hub.docker.com/r/pnrxa/openposterdb) | [GitHub Packages](https://ghcr.io/pnrxa/openposterdb)

## Quick Start

A hosted version is available at [openposterdb.com](https://openposterdb.com) — no setup required, just grab a free API key and point your media server at it.

To self-host, OpenPosterDB runs as a single Docker container.

### Docker Compose

```bash
# Copy the example env to the project root and fill in your API keys
cp api/.env.example .env
# Edit .env — at minimum set TMDB_API_KEY, MDBLIST_API_KEY (or OMDB), and JWT_SECRET

# Build and start
docker compose up -d
```

### Docker

```bash
docker run -d \
  -p 3000:3000 \
  -e TMDB_API_KEY=your_key \
  -e MDBLIST_API_KEY=your_key \
  -e JWT_SECRET=$(openssl rand -hex 32) \
  -v openposterdb-cache:/data/cache \
  -v openposterdb-db:/data/db \
  pnrxa/openposterdb
```

See [Configuration](#configuration) for additional environment variables.

### aiometadata

To use OpenPosterDB with [aiometadata](https://github.com/cedya77/aiometadata), set the following URL templates in your aiometadata configuration (replace `{api_key}` with your OpenPosterDB API key and `{base_url}` with your instance URL or `https://openposterdb.com`):

| Image type | URL template |
|---|---|
| Poster | `{base_url}/{api_key}/imdb/poster-default/{imdb_id}.jpg` |
| Background | `{base_url}/{api_key}/tmdb/backdrop-default/{type}-{tmdb_id}.jpg?imageSize=large` |
| Logo | `{base_url}/{api_key}/tmdb/logo-default/{type}-{tmdb_id}.png` |

`{type}` is `movie` or `series` depending on the media type.
| Episode | `{base_url}/{api_key}/imdb/episode-default/episode-{imdb_id}-S{season}E{episode}.jpg` |

TMDB and TVDB IDs also work for episodes:

| ID type | Episode URL template |
|---|---|
| TMDB | `{base_url}/{api_key}/tmdb/episode-default/episode-{tmdb_id}-S{season}E{episode}.jpg` |
| TVDB | `{base_url}/{api_key}/tvdb/episode-default/episode-{tvdb_id}-S{season}E{episode}.jpg` |

The episode endpoint accepts a series-level ID combined with season/episode numbers — no episode-specific ID is needed.

Season posters (a TV season's own poster art) use the `season-default` endpoint with a `season-{id}-S{season}` ID:

| ID type | Season URL template |
|---|---|
| IMDb | `{base_url}/{api_key}/imdb/season-default/season-{imdb_id}-S{season}.jpg` |
| TMDB | `{base_url}/{api_key}/tmdb/season-default/season-{tmdb_id}-S{season}.jpg` |
| TVDB | `{base_url}/{api_key}/tvdb/season-default/season-{tvdb_id}-S{season}.jpg` |

The ID is the **series** ID (IMDb/TMDB/TVDB) combined with the season number.

### Jellyfin

OpenPosterDB has a dedicated [Jellyfin plugin](https://github.com/PNRxA/jellyfin-plugin-openposterdb) — a separate, open-source remote image provider. It fetches posters, backdrops, logos and episode stills (with rating badges) from your self-hosted instance, keyed off each item's IMDb / TMDB / TVDB id. Install it from the plugin repository, then point it at your instance:

```
https://raw.githubusercontent.com/PNRxA/jellyfin-plugin-openposterdb/main/manifest.json
```

1. In Jellyfin, go to Dashboard → Plugins → Repositories and add the manifest URL above.
2. Install "OpenPosterDB" from the Catalog, then restart Jellyfin.
3. Open the plugin's settings and set the Base URL and API key for your OpenPosterDB instance.

See the [plugin repo](https://github.com/PNRxA/jellyfin-plugin-openposterdb) for version and compatibility details.

### Plex

Plex deprecated its legacy plugin agents, so OpenPosterDB connects through a **Custom Metadata Provider** (Plex Media Server **1.43+**). A hosted provider runs at **`https://plex.openposterdb.com`** — nothing to install. It's open source at [openposterdb-plex-provider](https://github.com/PNRxA/openposterdb-plex-provider) if you'd rather self-host.

Add it as two providers — one for movies, one for TV — each with your OpenPosterDB API key in the URL:

| Library | Provider URL |
|---|---|
| Movies | `https://plex.openposterdb.com/{api_key}/movie` |
| TV | `https://plex.openposterdb.com/{api_key}/tv` |

1. In Plex, go to Settings → Manage → Metadata Agents → **Add Provider** and add the Movies URL above; add the TV URL as a second provider.
2. **Add Agent** — create an agent that pairs the OpenPosterDB provider with **Plex Movie** (and another with **Plex TV Series**) so Plex still supplies titles and metadata, then drag OpenPosterDB above the default art source.
3. Assign the agent to each library under Library → Edit → Advanced → Agent, then **Refresh Metadata**.

OpenPosterDB then supplies rating-badged posters, backdrops and logos for your movies and TV shows. To pin a poster style, append OpenPosterDB [options](#api-endpoints) to the mount segment, e.g. `https://plex.openposterdb.com/{api_key}/movie~badge_style=v&ratings_limit=5`.

> [!NOTE]
> Plex's Custom Metadata Provider API is in beta and requires PMS 1.43+.

## API Endpoints

### Poster

```
GET /{api_key}/{id_type}/poster-default/{id_value}.jpg
```

- Returns JPEG with rating badges overlaid on the poster
- Uses TMDB (default) or Fanart.tv as the poster source

### Logo

```
GET /{api_key}/{id_type}/logo-default/{id_value}.png
```

- Returns transparent PNG with rating badges stacked below the logo
- Uses TMDB (default) or Fanart.tv as the image source

### Backdrop

```
GET /{api_key}/{id_type}/backdrop-default/{id_value}.jpg
```

- Returns JPEG with rating badges overlaid on the backdrop (configurable position and direction)
- Uses TMDB (default) or Fanart.tv as the image source

### Episode

```
GET /{api_key}/{id_type}/episode-default/{id_value}.jpg
```

- Returns JPEG with per-episode ratings on an episode still image (landscape)
- Falls back to the series poster when no episode still is available
- Supports episode IMDb IDs (e.g. `tt0959621`), TMDB episode format (`episode-1396-S1E1`), TVDB episode IDs, and series-level ID + season/episode for all ID types (e.g. `episode-tt14786934-S1E1` for IMDb, `episode-81189-S3E5` for TVDB)
- Dedicated episode settings: position, direction, badge style/size, label style, ratings limit
- `?blur=true` applies Gaussian blur for spoiler protection (badges remain sharp)
- IMDB episode ratings require an OMDb API key (MDBList does not support episode-level ratings)

### Season

```
GET /{api_key}/{id_type}/season-default/{id_value}.jpg
```

- Returns a JPEG season poster (2:3 portrait) with rating badges — a TV season's own poster art
- ID format is `season-{id}-S{season}` using the **series** IMDb/TMDB/TVDB ID (e.g. `season-1396-S2` for TMDB, `season-tt0903747-S2` for IMDb, `season-81189-S2` for TVDB)
- Uses the season's own poster from TMDB (or Fanart.tv season posters), falling back to the series poster when the season has no art of its own
- Season ratings are season-scoped where the source supports it — TMDB (the season's vote average) and Trakt (the season rating). IMDb/RT/Metacritic fall back to show-level ratings; MDBList is not queried for seasons
- Dedicated season settings: position, direction, badge style/size, label style, shape, background, ratings limit (mirrors poster controls; no blur)
- Renders through the same 2:3 poster pipeline, so it accepts the poster image sizes (`small` is not valid)

### Key Validation

```
GET /{api_key}/isValid
```

- Returns `200 OK` if the API key is valid, `401 Unauthorized` otherwise
- Compatible with RPDB integrations that validate keys before use

**Common parameters:**

- `id_type`: `imdb`, `tmdb`, `tvdb`
- `id_value`: e.g. `tt1234567`, `movie-123`, `series-456`, `episode-1396-S1E1`, `season-1396-S2`. Episode IMDb IDs (e.g. `tt0959621`), TVDB episode IDs, series-level IDs with season/episode (e.g. `episode-tt14786934-S1E1`, `episode-81189-S3E5`), and season IDs (e.g. `season-tt0903747-S2`, `season-81189-S2`) are also supported
- `?fallback=true`: accepted for RPDB plugin compatibility but ignored as OPDB falls back to TMDB by default
- `?lang={code}`: override the image language for this request (e.g. `?lang=de` for German, `?lang=pt-BR` for Brazilian Portuguese). Supports regional variants — when a region-specific image exists (e.g. `pt-BR`), it is preferred; otherwise falls back to the base language (`pt`), then English. Applies to posters and logos. Backdrops are language-agnostic and ignore this parameter
- `?imageSize={size}`: control output image dimensions. Available sizes vary by image type (see [Image Sizes](#image-sizes))
- `?ratings_limit={0-10}`: maximum number of rating badges to display (0 = no ratings)
- `?ratings_order={keys}`: comma-separated rating source keys controlling display order. Valid keys: `imdb`, `tmdb`, `rt` (RT Critics), `rta` (RT Audience), `mc` (Metacritic), `trakt`, `lb` (Letterboxd), `mal` (MyAnimeList), `mdblist` (MDBList score), `ebert` (Roger Ebert). Example: `?ratings_order=imdb,tmdb,rt`
- `?ratings_exclude={keys}`: comma-separated rating source keys to hide entirely (same valid keys as `ratings_order`). Excluded sources are dropped *before* ordering and limiting, so an excluded source frees its badge slot for the next preferred source rather than leaving a gap. Example: `?ratings_exclude=rt` shows your ratings but never RT Critics
- `?badge_style={h|v|d}`: badge layout — `h` (horizontal), `v` (vertical), `d` (default)
- `?label_style={t|i|o}`: label rendering — `t` (text), `i` (icon), `o` (official provider logos)
- `?badge_size={xs|s|m|l|xl}`: badge scale — extra-small, small, medium, large, extra-large
- `?badge_shape={r|p}`: badge corner shape — `r` (rounded, default), `p` (pill, fully rounded ends). Pills always render as a horizontal icon/label-left, value-right lozenge, even on image types whose default style is vertical (logos, backdrops, episodes)
- `?badge_background={d|k|t|n}`: badge background — `d` (default: source-coloured label + dark value), `k` (dark: uniformly dark), `t` (transparent: semi-transparent so the artwork shows through), `n` (none: no background, label/value drawn directly on the image with a drop shadow)
- `?image_source={t|f}`: image source — `t` (TMDB, default), `f` (Fanart.tv). Applies to all image types. The non-selected source is used as fallback
- `?badge_direction={d|h|v}`: badge stacking direction — `d` (default), `h` (horizontal), `v` (vertical). Applies to poster, backdrop, and episode endpoints
- `?position={bc|tc|l|r|tl|tr|bl|br}`: badge anchor position. Applies to poster, backdrop, and episode endpoints
- `?textless={true|false}`: prefer textless images when available (poster only). Works with both TMDB and Fanart.tv sources
- `?blur={true|false}`: apply Gaussian blur for spoiler protection (episode only). Badges remain sharp over the blurred still image
- `?split={true|false}`: split the badges evenly across two opposite sides of the poster (poster only). The axis follows the badge layout — a vertical layout splits left/right, horizontal rows split top/bottom. With an odd number of badges the configured side gets the extra one (e.g. 4 badges → 2 + 2, 3 badges → 2 + 1)
- `?fit={native|cover|pad|blur}`: how a poster is fit to the standard 2:3 output frame (poster only). `native` (default) keeps the source aspect ratio; `cover` scales to fill 2:3 and center-crops the overflow; `pad` fits the whole poster inside 2:3 with solid black bars; `blur` fits the whole poster inside 2:3 and fills the bars with a blurred, zoomed copy of the poster. The non-native modes guarantee a uniform 2:3 image so downstream apps that place posters in fixed 2:3 containers don't crop the art
- `?edge_inset_x={0-50}` / `?edge_inset_y={0-50}`: inset the backdrop ratings further from the anchored edge, as a percentage of the backdrop's width/height (backdrop only). Useful when a media player crops the backdrop and clips the ratings. `edge_inset_x` only applies to left/right positions and `edge_inset_y` only to top/bottom positions; the inset for a centered axis is ignored. Example: `?position=tr&edge_inset_y=10` nudges top-right ratings down by 10% of the height
- RPDB-compatible — use `http://localhost:3000` as the base URL (drop-in replacement for `https://api.ratingposterdb.com`). Old parameter names `?poster_source=` and `?fanart_textless=` are accepted as aliases

`textless`, `split`, and `fit` are poster-only. `blur` is episode-only. `edge_inset_x`/`edge_inset_y` are backdrop-only. `badge_direction` and `position` are silently ignored on logo endpoints. For shared parameters (`ratings_limit`, `badge_style`, `label_style`, `badge_size`, `badge_shape`, `badge_background`, `image_source`), the override is applied to the correct image-type-specific setting (e.g. `?badge_style=h` on the poster endpoint sets `poster_badge_style`, on the logo endpoint sets `logo_badge_style`).

Management endpoints (auth, keys, settings) are under `/api/` and return JSON.

### Image Sizes

The `?imageSize=` parameter controls the output dimensions. When omitted, `medium` is used as the default. All badge elements scale proportionally with the image.

**Poster sizes:**

| Size | Dimensions |
|---|---|
| `medium` *(default)* | 580 × 859 |
| `large` | 1280 × 1896 |
| `very-large` / `verylarge` | 2000 × 2962 |

Heights are representative for a standard ~2:3 source under the default `native` fit, which preserves the source aspect ratio (so the exact height varies per poster). The `cover`, `pad`, and `blur` fits instead produce an exact 2:3 frame (580 × 870, 1280 × 1920, 2000 × 3000).

Season posters use the same sizes as posters (they share the 2:3 poster pipeline).

**Logo sizes:**

| Size | Dimensions |
|---|---|
| `medium` *(default)* | 780 × 244 |
| `large` | 1722 × 539 |
| `very-large` / `verylarge` | 2689 × 841 |

**Backdrop sizes:**

| Size | Dimensions |
|---|---|
| `small` | 1280 × 720 |
| `medium` *(default)* | 1920 × 1080 |
| `large` | 3840 × 2160 |

**Episode sizes:**

| Size | Dimensions |
|---|---|
| `small` | 480 × 270 |
| `medium` *(default)* | 780 × 439 |
| `large` | 1280 × 720 |
| `very-large` / `verylarge` | 1920 × 1080 |

`small` is only valid for backdrops and episodes — requesting it for posters or logos returns `400 Bad Request`. `verylarge` is accepted as an alias for `very-large` for RPDB compatibility.

## Features

- **Multi-source ratings** — Aggregates from MDBList (IMDb, RT Critics, RT Audience, Metacritic, Trakt, Letterboxd, MAL, the MDBList score, and Roger Ebert), optionally OMDb, and optionally Trakt directly (for episode-level ratings and as a standalone provider)
- **Multiple image sources** — Uses TMDB as the primary source for posters, logos, and backdrops, with Fanart.tv as an optional fallback (or preferred source). Supports language selection and textless posters from both sources
- **Configurable per API key** — Override image source, language, and textless settings per key, or set global defaults
- **ID resolution** — Accepts IMDb, TMDB, or TVDB IDs
- **Multi-layer caching** — In-memory (moka), filesystem, and SQLite metadata with background refresh and request coalescing
- **Admin UI** — Vue 3 web panel for API key management, poster settings, and global configuration
- **Auth** — Argon2 password hashing, JWT access tokens, rotating refresh tokens, API key access for poster endpoints

## Tech Stack

- **API**: Rust, Axum, SeaORM + SQLite, image/imageproc for rendering
- **Web**: Vue 3, TypeScript, Tailwind CSS, Vite

## Getting Started

### Docker

```bash
# Copy the example env to the project root and fill in your API keys
cp api/.env.example .env
# Edit .env — at minimum set TMDB_API_KEY, MDBLIST_API_KEY (or OMDB), and JWT_SECRET

# Build and start
docker compose up -d
```

### Without Docker

### Requirements

- Rust toolchain
- Node.js 20.19+ (for admin UI)
- A [TMDB API key](https://www.themoviedb.org/settings/api)
- At least one of: [MDBList API key](https://mdblist.com/preferences/) (preferred — covers all 9 rating sources), [OMDb API key](https://www.omdbapi.com/apikey.aspx), [Trakt Client ID](https://trakt.tv/oauth/applications)
- Optional: [Fanart.tv API key](https://fanart.tv/get-an-api-key/) (enables Fanart.tv as an alternative or preferred image source)


### API

```bash
cd api
cp .env.example .env
# Edit .env — at minimum set TMDB_API_KEY, MDBLIST_API_KEY (or OMDB), and JWT_SECRET
cargo run --release
```

### Web UI

```bash
cd web
npm install
npm run dev        # development
npm run build      # production
```

The web UI will be available at `http://localhost:3000`. On first visit you'll be prompted to create an admin account.

If you access the UI over plain HTTP (no reverse proxy with TLS), add `COOKIE_SECURE=false` to your `.env` — otherwise the browser will silently drop auth cookies and login will appear broken.

See [docker-compose.yml](docker-compose.yml) for the full compose configuration.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `TMDB_API_KEY` | *required* | TMDB API v3 key |
| `JWT_SECRET` | *required* | 32-byte hex string (`openssl rand -hex 32`) |
| `MDBLIST_API_KEY` | — | MDBList key — preferred, covers all 9 rating sources (IMDb, RT Critics, RT Audience, Metacritic, Trakt, Letterboxd, MAL, MDBList score, Roger Ebert) |
| `OMDB_API_KEY` | — | OMDb key (IMDb, RT Critics, Metacritic only) |
| `TRAKT_CLIENT_ID` | — | [Trakt](https://trakt.tv/oauth/applications) Client ID — provides Trakt community ratings for movies, shows, and episodes |
| `LISTEN_ADDR` | `0.0.0.0:3000` | Server bind address |
| `CACHE_DIR` | `./cache` | Poster and metadata cache directory |
| `DB_DIR` | `./db` | SQLite database directory |
| `IMAGE_QUALITY` | `85` | JPEG output quality (1-100) |
| `IMAGE_MEM_CACHE_MB` | `512` | In-memory cache size in MB |
| `RATINGS_STALE_SECS` | `86400` | Min ratings cache lifetime |
| `RATINGS_MAX_AGE_SECS` | `31536000` | Film age after which ratings stop refreshing |
| `IMAGE_STALE_SECS` | `0` | Base image cache lifetime (0 = never re-fetch) |
| `COOKIE_SECURE` | `true` | HTTPS-only cookies |
| `RUST_LOG` | `warn` | Log level filter — levels: `error`, `warn`, `info`, `debug`, `trace`. Supports comma-separated per-module overrides. Relevant modules: `openposterdb_api` (app), `tower_http` (HTTP tracing), `sea_orm` / `sqlx` (database), `reqwest` / `hyper` (HTTP client/server). Example: `warn,openposterdb_api=info,tower_http=debug` |
| `FANART_API_KEY` | — | [Fanart.tv](https://fanart.tv/get-an-api-key/) key (enables Fanart.tv as an alternative or preferred image source for posters, logos, and backdrops) |
| `CORS_ORIGIN` | — | Allowed origin for admin requests |
| `RENDER_CONCURRENCY` | `CPUs × 2` | Max concurrent image render tasks |
| `CROSS_ID_CONCURRENCY` | `CPUs` | Max concurrent cross-ID cache write tasks |
| `ADMIN_USERNAME` | — | Seed admin username on first run |
| `ADMIN_PASSWORD` | — | Seed admin password on first run |
| `ENABLE_CDN_REDIRECTS` | `false` | Enable content-addressed CDN redirects (see [CDN Caching](#cdn-caching)) |
| `EXTERNAL_CACHE_ONLY` | `false` | Skip image file writes to disk; rely on a CDN for caching. SQLite metadata is still written (see [External Cache Only](#external-cache-only)) |
| `FREE_KEY_ENABLED` | — | Force-enable (`true`) or force-disable (`false`) the free API key, overriding the admin UI toggle. When set, the UI toggle is locked. Omit to let admins control it from the settings page |
| `DISABLE_PUBLIC_PAGES` | `false` | Hide the landing page, docs, legal, and all unauthenticated routes, redirecting visitors to the login page instead. All pages remain accessible to authenticated users. Useful for private self-hosted instances that aren't intended to be publicly accessible |

## Cache Architecture

Images are cached in three layers: in-memory (moka), filesystem, and SQLite metadata. Cache keys encode all the settings that affect the rendered output so that different configurations produce separate cached files.

### Filesystem Layout

```
{CACHE_DIR}/
├── base/
│   ├── posters/{tmdb_size}/  # Raw TMDB poster downloads (grouped by CDN size: w500, w780, original)
│   └── fanart/           # Raw fanart.tv downloads ({fanart_id}.{ext})
├── posters/{id_type}/    # Rendered poster JPEGs
├── logos/{id_type}/       # Rendered logo PNGs
├── backdrops/{id_type}/   # Rendered backdrop JPEGs
├── episodes/{id_type}/   # Rendered episode JPEGs
└── preview/{subdir}/      # Preview images for the settings UI
```

### Cache Key Format

Cache keys uniquely identify a rendered image. They are used as keys in the in-memory cache and stored in the `image_meta` SQLite table.

**Poster:**
```
{id_type}/{id_value}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}{badge_size_suffix}{shape_suffix}{background_suffix}{split_suffix}{fit_suffix}{size_suffix}
```

**Fanart poster:**
```
{id_type}/{id_value}{variant}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}{badge_size_suffix}{shape_suffix}{background_suffix}{split_suffix}{fit_suffix}{size_suffix}
```

**Logo:**
```
{id_type}/{id_value}{kind_prefix}{variant}{ratings_suffix}{style_suffix}{label_suffix}{badge_size_suffix}{shape_suffix}{background_suffix}{size_suffix}
```

**Backdrop:**
```
{id_type}/{id_value}{kind_prefix}{variant}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}{badge_size_suffix}{shape_suffix}{background_suffix}{edge_inset_suffix}{size_suffix}
```

### Suffix Reference

| Suffix | Format | Example | Description |
|---|---|---|---|
| Ratings | `@{chars}` | `@mil` | Single-char per source, no commas (`m`=MAL, `i`=IMDb, `l`=Letterboxd, `r`=RT, `a`=RT Audience, `c`=Metacritic, `t`=TMDB, `k`=Trakt, `d`=MDBList score, `e`=Roger Ebert) |
| Position | `.p{pos}` | `.pbc`, `.pl` | Poster badge position (`bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br`) |
| Badge style | `.s{style}` | `.sh`, `.sv` | `h` = horizontal, `v` = vertical |
| Label style | `.l{style}` | `.lt`, `.li`, `.lo` | `t` = text labels, `i` = icon labels, `o` = official provider logos |
| Badge direction | `.d{dir}` | `.dh`, `.dv` | `h` = horizontal, `v` = vertical (resolved from `d` = default) |
| Badge size | `.b{size}` | `.bm`, `.bxl` | `xs` = extra-small, `s` = small, `m` = medium (default), `l` = large, `xl` = extra-large |
| Badge shape | `.sh{shape}` | `.shr`, `.shp` | `r` = rounded (default), `p` = pill (the `sh` prefix distinguishes it from the `.s{style}` token above) |
| Badge background | `.bg{bg}` | `.bgd`, `.bgn` | `d` = default, `k` = dark, `t` = transparent, `n` = none |
| Split (poster) | `.x1` | `.x1` | Poster badges split onto opposite sides; only present when enabled |
| Poster fit | `.f{fit}` | `.fc`, `.fp`, `.fb` | `c` = cover, `p` = pad, `b` = blur — `native` (default) emits no token |
| Edge inset (backdrop) | `.eh{n}` / `.ev{n}` | `.eh8`, `.ev3` | Backdrop ratings inset from the edge by `n`% — `eh` horizontal, `ev` vertical; only the position-relevant axis, only when non-zero |
| Image size | `.z{size}` | `.zm`, `.zl` | `s` = small, `m` = medium (default), `l` = large, `vl` = very-large |

### Image Kind Prefixes

Logos, backdrops, and episodes include a kind prefix in their cache keys to distinguish them from posters:

| Kind | Prefix |
|---|---|
| Poster | *(none)* |
| Logo | `_l` |
| Backdrop | `_b` |
| Episode | `_e` |
| Season | `_s` |

### Source Variant Markers

Logos and backdrops include a source marker (`_t` for TMDB, `_f` for Fanart.tv) to distinguish images from different sources. Posters use a separate variant scheme — the default case (English, non-textless) has no marker for backward compatibility:

| Image type | Variant | Marker | Description |
|---|---|---|---|
| Poster | TMDB default | *(none)* | Default English poster from TMDB (backward-compatible) |
| Poster | TMDB language | `_t_{lang}` | Language-specific TMDB poster (e.g. `_t_de`) |
| Poster | TMDB textless | `_t_tl` | Textless TMDB poster |
| Poster | Fanart textless | `_f_tl` | Fanart image with no text overlay |
| Poster | Fanart language | `_f_{lang}` | Fanart image matching language (e.g. `_f_en`) |
| Logo/Backdrop | TMDB | `_t` or `_t_{lang}` | Image sourced from TMDB |
| Logo/Backdrop | Fanart | `_f` or `_f_{lang}` | Image sourced from Fanart.tv |
| *(negative)* | Textless miss | `_f_tl_neg` | No textless fanart image available |
| *(negative)* | Language miss | `_f_{lang}_neg` | No fanart image for this language |

### Database Values

The `image_meta` table tracks metadata for cached images:

| Field | Short Value | Meaning |
|---|---|---|
| `image_type` | `p` | Poster |
| `image_type` | `l` | Logo |
| `image_type` | `b` | Backdrop |
| `image_type` | `e` | Episode |
| `image_type` | `s` | Season |

### Settings Short Values

Settings are stored as short single-character or two-character codes:

| Setting | Values | Meaning |
|---|---|---|
| `image_source` | `t`, `f` | TMDB, Fanart.tv |
| `badge_style` | `h`, `v` | Horizontal, Vertical |
| `label_style` | `t`, `i`, `o` | Text, Icon, Official |
| `badge_direction` | `d`, `h`, `v` | Default (auto-resolved by position), Horizontal, Vertical |
| `badge_size` | `xs`, `s`, `m`, `l`, `xl` | Extra-small (0.5×), Small (0.75×), Medium (1.0×), Large (1.25×), Extra-large (1.5×) |
| `badge_shape` | `r`, `p` | Rounded (default), Pill |
| `badge_background` | `d`, `k`, `t`, `n` | Default (coloured label + dark value), Dark, Transparent, None |
| `position` | `bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br` | Bottom-center, Top-center, Left, Right, corners |

### Example Cache Keys

```
# TMDB poster, 3 ratings (MAL, IMDb, Letterboxd), bottom-center, horizontal badges, official labels, horizontal direction, medium badge size, medium image
imdb/tt0111161@mil.pbc.sh.lo.dh.bm.zm

# Same poster at large image size with large badge size
imdb/tt0111161@mil.pbc.sh.lo.dh.bl.zl

# Fanart textless poster
imdb/tt0111161_f_tl@mil.pbc.sh.lo.dh.bm.zm

# Logo from TMDB with English language, 3 ratings, horizontal badges, text labels
imdb/tt0111161_l_t_en@mil.sh.lt.bm.zm

# Logo from Fanart.tv with English language
imdb/tt0111161_l_f_en@mil.sh.lt.bm.zm

# Backdrop from TMDB with top-right position, vertical direction, vertical badges, official labels, extra-large badge size, large image
imdb/tt0111161_b_t@mil.ptr.sv.lo.dv.bxl.zl

# Episode with 1 rating, top-right position, vertical direction, vertical badges, official labels, medium badge size, blur enabled
imdb/tt0959621_e@i.ptr.sv.lo.dv.bm.blur.zm
```

### Cross-ID Cache

When a poster is generated via one ID type (e.g. IMDB), the rendered image is also written to the filesystem cache under all resolved alternate IDs (TMDB, TVDB). This avoids redundant image generation when the same content is requested via different ID types.

- Alternate IDs are determined from the moka-cached `ResolvedId` (no extra API calls)
- Writes are best-effort and parallelized — errors are logged but not propagated
- Only the filesystem cache and DB metadata are populated; the in-memory cache is not — alternate keys get promoted to memory on their first actual request
- Applies to all image types: posters, logos, and backdrops

### Staleness and Background Refresh

Cache entries are checked for staleness based on the film's release date:
- **Unreleased / unknown**: uses `RATINGS_STALE_SECS` (default 24h)
- **Recent films**: linearly increasing stale time from `RATINGS_STALE_SECS` to `RATINGS_MAX_AGE_SECS`
- **Old films** (age > `RATINGS_MAX_AGE_SECS`): never stale (ratings are stable)

When a stale entry is served, a background refresh is spawned to regenerate it without blocking the response. Request coalescing ensures concurrent requests for the same image share a single generation task.

### CDN Caching

When `ENABLE_CDN_REDIRECTS=true`, authenticated poster requests (`/{api_key}/...`) return a **302 redirect** to a content-addressed URL (`/c/{settings_hash}/...`) instead of serving the image directly. This is designed for deployments behind Cloudflare or another CDN:

1. The app computes a 32-character hex hash from the user's effective settings (ratings order, badge style, position, etc.)
2. The original endpoint validates the API key, then redirects to `/c/{hash}/{id_type}/poster-default/{id_value}.jpg`
3. The `/c/` endpoint serves the image with a dynamic `Cache-Control` TTL based on the film's age (see below)
4. The CDN caches by the `/c/` URL — all users with identical settings share one cache entry

**Why this helps:** Without redirects, the CDN caches by the full URL including the API key, so two users requesting the same poster with the same settings produce two separate cache entries. With redirects, they share one.

**When to enable:** Only when a CDN sits in front of the origin. Without a CDN, the redirect is an extra round-trip to the same server for no benefit.

The redirect response uses `Cache-Control: public, max-age=300, stale-while-revalidate=3600` so the CDN caches the redirect at the edge. The cache is keyed by the full URL (which includes the API key), so one user's cached redirect is never served to another. The `stale-while-revalidate` directive allows the edge to keep serving the cached redirect for up to an hour while the origin is unreachable. The `/c/` image response uses a dynamic `Cache-Control` TTL that scales with the film's age — the same staleness logic used for internal cache revalidation:

| Film age | `max-age` | Why |
|---|---|---|
| Unreleased / unknown | `RATINGS_STALE_SECS` (default 1 day) | Ratings are volatile, may change daily |
| Recently released | Scales linearly from 1 day to 1 year | Ratings stabilize over time |
| Older than `RATINGS_MAX_AGE_SECS` (default 1 year) | 1 year | Ratings are settled |

The `stale-while-revalidate` directive is set to 7x the `max-age`, so CDN edge nodes can serve slightly stale content while revalidating in the background. The `/c/` routes are rate-limited by IP.

**Important:** The origin keeps the settings hash → settings mapping in memory with a 5-minute TTL. The CDN must cache the image on the first request to the `/c/` URL; if it doesn't, subsequent requests after the TTL expires will 404 at origin until the next authenticated request re-populates the mapping. Cloudflare and most production CDNs cache on first hit, so this is not an issue in practice.

### External Cache Only

When `EXTERNAL_CACHE_ONLY=true`, the server skips image file writes to disk (rendered posters and base source images from TMDB/Fanart.tv). This is useful when deployed behind a CDN like Cloudflare that caches responses at the edge.

- The in-memory (moka) cache still handles short-term request deduplication
- Request coalescing still prevents duplicate generation for concurrent requests
- The cache directory is not created on startup
- Filesystem reads naturally return misses (no files on disk), so every request either hits the in-memory cache or regenerates the image
- Best used together with `ENABLE_CDN_REDIRECTS=true` so the CDN absorbs the vast majority of traffic
- SQLite metadata is **always** written, even with this flag — `image_meta` stores release dates (for CDN TTL computation) and `available_ratings` records which rating sources have data for each movie (so cache keys can be reconstructed without external API calls on cache hits)
- The Docker volume is still required for the SQLite database (`DB_DIR`), even when image caching is fully external

## Deploying to the Public Internet

If you plan to expose OpenPosterDB to external users, put it behind a reverse proxy with TLS and (optionally) a CDN. The sections below cover using Caddy as the reverse proxy and Cloudflare as the CDN.

### Reverse Proxy with Caddy

The repository includes a [`Caddyfile.example`](Caddyfile.example). Copy it, replace the domain, and add a Caddy service to your compose file:

```yaml
services:
  caddy:
    image: caddy:2
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
    restart: unless-stopped

  openposterdb:
    image: pnrxa/openposterdb:latest
    environment:
      # refer to docker-compose.yml
    volumes:
      - openposterdb-data:/data
    restart: unless-stopped

volumes:
  caddy-data:
  openposterdb-data:
```

Caddy automatically provisions TLS certificates via Let's Encrypt. No extra configuration is needed — just point your DNS A record to the server's IP.

### Deploying behind Cloudflare

When Cloudflare sits in front of your origin, you can enable two environment flags that significantly reduce origin load:

```env
ENABLE_CDN_REDIRECTS=true
EXTERNAL_CACHE_ONLY=true
```

- **`ENABLE_CDN_REDIRECTS`** makes poster requests redirect to content-addressed `/c/` URLs so Cloudflare deduplicates cache entries across users with identical settings (see [CDN Caching](#cdn-caching))
- **`EXTERNAL_CACHE_ONLY`** skips image file writes to disk, relying on Cloudflare's edge cache for long-term storage and the in-memory cache for short-term deduplication. SQLite metadata (release dates, available rating sources) is still written so cache keys and CDN TTLs can be computed without external API calls

Use the same Caddy + OpenPosterDB compose setup from the [reverse proxy section](#reverse-proxy-with-caddy), adding the two CDN flags and bumping the in-memory cache. The key changes to the `openposterdb` environment:

```yaml
      # Add these to your existing environment block
      ENABLE_CDN_REDIRECTS: "true"
      EXTERNAL_CACHE_ONLY: "true"
      IMAGE_MEM_CACHE_MB: ${IMAGE_MEM_CACHE_MB:-1024}
```

> `CACHE_DIR` can be omitted because no images are written to disk. The volume is still needed for the SQLite database (`DB_DIR`). `IMAGE_MEM_CACHE_MB` is increased to 1024 because the in-memory cache is the only deduplication layer before Cloudflare — size it to fit your server's available RAM.

#### Cloudflare configuration

1. **DNS**: Add an A record pointing to your server's IP with the orange cloud (Proxied) enabled.

2. **SSL/TLS**: Go to **SSL/TLS > Overview** and set the mode to **Full (strict)**. This encrypts traffic between Cloudflare and your origin. Caddy's auto-TLS handles the origin certificate, or you can use a [Cloudflare Origin CA certificate](https://developers.cloudflare.com/ssl/origin-configuration/origin-ca/).

3. **Cache Rules**: Cloudflare already caches `.jpg` and `.png` responses by default. The origin sets a dynamic `Cache-Control` header based on film age (1 day for new releases, up to 1 year for older films). To ensure Cloudflare respects this, add a cache rule:
   - Go to **Caching > Cache Rules** and create a rule:
     - **When**: URI Path starts with `/c/`
     - **Then**: **Eligible for cache**, set **Edge TTL** to **Respect origin**
   - This ensures new releases get short edge TTLs (so updated ratings propagate quickly) while old films stay cached for up to a year.

4. **Tiered Cache**: Go to **Caching > Tiered Cache** and enable **Smart Tiered Caching**. This reduces origin hits by allowing Cloudflare's upper-tier data centers to serve cache hits to lower-tier ones.

5. **Static Assets**: The web UI's static assets (JS, CSS, fonts, images) are built by Vite with content hashes in their filenames (e.g. `assets/index-abc123.js`), making them safe to cache aggressively. Add a second cache rule:
   - **When**: URI Path starts with `/assets/`
   - **Then**: **Eligible for cache**, set **Edge TTL** to 1 year, **Browser TTL** to 1 year
   - These files are immutable — when the app is redeployed, Vite generates new filenames, so stale cache entries are never served.
   - The SPA's `index.html` is served without a file extension on all non-API routes, so Cloudflare will not cache it by default without a rule (see below).

6. **Web Console (origin-down resilience)**: The origin sets `Cache-Control: public, max-age=60, stale-while-revalidate=3600, stale-if-error=86400` on all SPA responses (HTML and static files). Add a cache rule to let Cloudflare respect these headers for the HTML shell:
   - **When**: Hostname equals the domain **AND** URI Path does not start with `/api/` **AND** URI Path does not start with `/c/` **AND** URI Path does not start with `/assets/`
   - **Then**: **Eligible for cache**, **Edge TTL** = **Respect origin**
   - This keeps `index.html` fresh (60 s browser TTL) during normal operation, but allows Cloudflare to serve a stale copy for up to 24 hours (`stale-if-error`) when the origin is unreachable — making the full web console available from cache during outages. Hashed `/assets/` files are already cached by the rule above.

7. **Browser TTL** (optional): Under **Caching > Configuration**, set **Browser Cache TTL** to **Respect Existing Headers** so the origin's `Cache-Control` headers are passed through to clients.

#### How the CDN flow works

```
Client → Cloudflare edge
  → /{api_key}/imdb/poster-default/tt1234567.jpg
  → Origin validates API key, returns 302 → /c/{hash}/imdb/poster-default/tt1234567.jpg
  → Cloudflare follows redirect internally (or client follows it)
  → /c/{hash}/... → Cache HIT at edge (served from Cloudflare)
     or Cache MISS → Origin renders image → Cloudflare caches it → Response
```

After the first request, all users with the same settings get the cached image directly from Cloudflare's edge — the origin is not hit.

## Acknowledgments

OpenPosterDB is made possible by these third-party services and projects:

### Data & Image Providers

- **[TMDB (The Movie Database)](https://www.themoviedb.org/)** ([get API key](https://www.themoviedb.org/settings/api)) — Movie and TV metadata, poster images. This product uses the TMDB API but is not endorsed or certified by TMDB.
- **[MDBList](https://mdblist.com/)** ([get API key](https://mdblist.com/preferences/)) — Aggregated ratings from multiple sources in a single API
- **[OMDb (Open Movie Database)](https://www.omdbapi.com/)** ([get API key](https://www.omdbapi.com/apikey.aspx)) — Alternative ratings source for IMDb, Rotten Tomatoes, and Metacritic
- **[Fanart.tv](https://fanart.tv/)** ([get API key](https://fanart.tv/get-an-api-key/)) — Optional alternative source for high-quality fan art, logos, and backdrops with language and textless support
- **[RPDB (Rating Poster Database)](https://ratingposterdb.com/)** — The original project that inspired OpenPosterDB's API design
- **[Simple Icons](https://simpleicons.org/)** — SVG icons for popular brands

### Rating Sources

- **[IMDb](https://www.imdb.com/)** — Internet Movie Database ratings
- **[Rotten Tomatoes](https://www.rottentomatoes.com/)** — Critics and audience scores
- **[Metacritic](https://www.metacritic.com/)** — Aggregated critic reviews
- **[Trakt](https://trakt.tv/)** — Community ratings and tracking
- **[Letterboxd](https://letterboxd.com/)** — Film community ratings
- **[MyAnimeList](https://myanimelist.net/)** — Anime and manga ratings
