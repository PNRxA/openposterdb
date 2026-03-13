> [!NOTE]
> This project is developed with the assistance of AI code generation tools. AI-generated code is reviewed and tested before being merged, but if you encounter any issues, please feel free to open an issue or submit a pull request.

# OpenPosterDB

A self-hosted, drop-in replacement for [RPDB (Rating Poster Database)](https://ratingposterdb.com). Generates movie and TV show posters, logos, and backdrops with rating badges from multiple sources overlaid on them. Fetches art from TMDB (or optionally [Fanart.tv](https://fanart.tv)), aggregates ratings from IMDb, Rotten Tomatoes, Metacritic, Trakt, Letterboxd, MyAnimeList, and composites color-coded badges onto the image.

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
- Requires `FANART_API_KEY` (returns 501 if not configured)

### Backdrop

```
GET /{api_key}/{id_type}/backdrop-default/{id_value}.jpg
```

- Returns JPEG with rating badges stacked vertically in the top-right corner
- Requires `FANART_API_KEY` (returns 501 if not configured)

### Key Validation

```
GET /{api_key}/isValid
```

- Returns `200 OK` if the API key is valid, `401 Unauthorized` otherwise
- Compatible with RPDB integrations that validate keys before use

**Common parameters:**

- `id_type`: `imdb`, `tmdb`, `tvdb`
- `id_value`: e.g. `tt1234567`, `movie-123`, `series-456`
- `?fallback=true`: return a placeholder image instead of an error on failure
- `?lang={code}`: override the Fanart.tv language for this request (e.g. `?lang=de` for German posters). Automatically switches the poster source to Fanart.tv even if TMDB is configured
- `?imageSize={size}`: control output image dimensions. Available sizes vary by image type (see [Image Sizes](#image-sizes))
- RPDB-compatible — use `http://localhost:3000` as the base URL (drop-in replacement for `https://api.ratingposterdb.com`)

Management endpoints (auth, keys, settings) are under `/api/` and return JSON.

### Image Sizes

The `?imageSize=` parameter controls the output dimensions. When omitted, `medium` is used as the default. All badge elements scale proportionally with the image.

**Poster sizes:**

| Size | Dimensions |
|---|---|
| `medium` *(default)* | 580 × 859 |
| `large` | 1280 × 1896 |
| `very-large` | 2000 × 2962 |

**Logo sizes:**

| Size | Dimensions |
|---|---|
| `medium` *(default)* | 780 × 244 |
| `large` | 1722 × 539 |
| `very-large` | 2689 × 841 |

**Backdrop sizes:**

| Size | Dimensions |
|---|---|
| `small` | 1280 × 720 |
| `medium` *(default)* | 1920 × 1080 |
| `large` | 3840 × 2160 |

`small` is only valid for backdrops — requesting it for posters or logos returns `400 Bad Request`. `verylarge` is accepted as an alias for `very-large` for RPDB compatibility.

## Features

- **Multi-source ratings** — Aggregates from MDBList (IMDb, RT Critics, RT Audience, Metacritic, Trakt, Letterboxd, MAL) and optionally OMDb
- **Alternative poster sources** — Use TMDB (default) or Fanart.tv with language preference and textless poster support
- **Configurable per API key** — Override poster source, language, and textless settings per key, or set global defaults
- **ID resolution** — Accepts IMDb, TMDB, or TVDB IDs
- **Multi-layer caching** — In-memory (moka), filesystem, and SQLite metadata with background refresh and request coalescing
- **Admin UI** — Vue 3 web panel for API key management, poster settings, and global configuration
- **Auth** — Argon2 password hashing, JWT access tokens, rotating refresh tokens, API key access for poster endpoints

## Tech Stack

- **API**: Rust, Axum, SeaORM + SQLite, image/imageproc for rendering
- **Web**: Vue 3, TypeScript, Tailwind CSS, Vite

## Quick Start

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
- At least one of: [MDBList API key](https://mdblist.com/preferences/) (preferred — covers all 7 rating sources), [OMDb API key](https://www.omdbapi.com/apikey.aspx)
- Optional: [Fanart.tv API key](https://fanart.tv/get-an-api-key/) (for alternative poster source with language/textless support)


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
| `MDBLIST_API_KEY` | — | MDBList key — preferred, covers all 7 rating sources (IMDb, RT Critics, RT Audience, Metacritic, Trakt, Letterboxd, MAL) |
| `OMDB_API_KEY` | — | OMDb key (IMDb, RT Critics, Metacritic only) |
| `LISTEN_ADDR` | `0.0.0.0:3000` | Server bind address |
| `CACHE_DIR` | `./cache` | Poster and metadata cache directory |
| `DB_DIR` | `./db` | SQLite database directory |
| `POSTER_QUALITY` | `85` | JPEG output quality (1-100) |
| `POSTER_MEM_CACHE_MB` | `512` | In-memory cache size in MB |
| `RATINGS_STALE_SECS` | `86400` | Min ratings cache lifetime |
| `RATINGS_MAX_AGE_SECS` | `31536000` | Film age after which ratings stop refreshing |
| `POSTER_STALE_SECS` | `0` | Base poster cache lifetime (0 = never re-fetch) |
| `COOKIE_SECURE` | `true` | HTTPS-only cookies |
| `RUST_LOG` | `warn` | Log level filter — levels: `error`, `warn`, `info`, `debug`, `trace`. Supports comma-separated per-module overrides (e.g. `warn,openposterdb=info` to keep dependencies quiet while showing app logs) |
| `FANART_API_KEY` | — | [Fanart.tv](https://fanart.tv/get-an-api-key/) key (enables Fanart.tv as alternative poster source; required for logo and backdrop endpoints) |
| `CORS_ORIGIN` | — | Allowed origin for admin requests |
| `RENDER_CONCURRENCY` | `CPUs × 2` | Max concurrent image render tasks |
| `CROSS_ID_CONCURRENCY` | `CPUs` | Max concurrent cross-ID cache write tasks |
| `ADMIN_USERNAME` | — | Seed admin username on first run |
| `ADMIN_PASSWORD` | — | Seed admin password on first run |
| `ENABLE_CDN_REDIRECTS` | `false` | Enable content-addressed CDN redirects (see [CDN Caching](#cdn-caching)) |
| `EXTERNAL_CACHE_ONLY` | `false` | Skip image file writes to disk; rely on a CDN for caching. SQLite metadata is still written (see [External Cache Only](#external-cache-only)) |
| `FREE_KEY_ENABLED` | — | Force-enable (`true`) or force-disable (`false`) the free API key, overriding the admin UI toggle. When set, the UI toggle is locked. Omit to let admins control it from the settings page |

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
└── preview/{subdir}/      # Preview images for the settings UI
```

### Cache Key Format

Cache keys uniquely identify a rendered image. They are used as keys in the in-memory cache and stored in the `poster_meta` SQLite table.

**Poster:**
```
{id_type}/{id_value}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}{size_suffix}
```

**Fanart poster:**
```
{id_type}/{id_value}{variant}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}{size_suffix}
```

**Logo / Backdrop:**
```
{id_type}/{id_value}{kind_prefix}{variant}{ratings_suffix}{style_suffix}{label_suffix}{size_suffix}
```

### Suffix Reference

| Suffix | Format | Example | Description |
|---|---|---|---|
| Ratings | `@{chars}` | `@mil` | Single-char per source, no commas (`m`=MAL, `i`=IMDb, `l`=Letterboxd, `r`=RT, `a`=RT Audience, `c`=Metacritic, `t`=TMDB, `k`=Trakt) |
| Position | `.p{pos}` | `.pbc`, `.pl` | Poster badge position (`bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br`) |
| Badge style | `.s{style}` | `.sh`, `.sv` | `h` = horizontal, `v` = vertical |
| Label style | `.l{style}` | `.lt`, `.li` | `t` = text labels, `i` = icon labels |
| Badge direction | `.d{dir}` | `.dh`, `.dv` | `h` = horizontal, `v` = vertical (resolved from `d` = default) |
| Image size | `.z{size}` | `.zm`, `.zl` | `s` = small, `m` = medium (default), `l` = large, `vl` = very-large |

### Image Kind Prefixes

Logos and backdrops include a kind prefix in their cache keys to distinguish them from posters:

| Kind | Prefix |
|---|---|
| Poster | *(none)* |
| Logo | `_l` |
| Backdrop | `_b` |

### Fanart Variant Markers

When the poster source is fanart.tv, the cache key includes a variant marker indicating which fanart tier was used:

| Variant | Marker | Description |
|---|---|---|
| Textless | `_f_tl` | Fanart image with no text overlay |
| Language | `_f_{lang}` | Fanart image matching language (e.g. `_f_en`) |
| Negative (textless) | `_f_tl_neg` | No textless image available (stored in negative cache) |
| Negative (language) | `_f_{lang}_neg` | No language image available |

### Database Values

The `poster_meta` table tracks metadata for cached images:

| Field | Short Value | Meaning |
|---|---|---|
| `image_type` | `p` | Poster |
| `image_type` | `l` | Logo |
| `image_type` | `b` | Backdrop |

### Settings Short Values

Settings are stored as short single-character or two-character codes:

| Setting | Values | Meaning |
|---|---|---|
| `poster_source` | `t`, `f` | TMDB, Fanart.tv |
| `badge_style` | `h`, `v` | Horizontal, Vertical |
| `label_style` | `t`, `i` | Text, Icon |
| `badge_direction` | `d`, `h`, `v` | Default (auto-resolved by position), Horizontal, Vertical |
| `poster_position` | `bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br` | Bottom-center, Top-center, Left, Right, corners |

### Example Cache Keys

```
# TMDB poster, 3 ratings (MAL, IMDb, Letterboxd), bottom-center, horizontal badges, icon labels, horizontal direction, medium (default)
imdb/tt0111161@mil.pbc.sh.li.dh.zm

# Same poster at large size
imdb/tt0111161@mil.pbc.sh.li.dh.zl

# Fanart textless poster
imdb/tt0111161_f_tl@mil.pbc.sh.li.dh.zm

# Logo with 3 ratings, horizontal badges, text labels
imdb/tt0111161_l_f_en@mil.sh.lt.zm

# Backdrop with vertical badges, icon labels, large size
imdb/tt0111161_b@mil.sv.li.zl
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
- SQLite metadata is **always** written, even with this flag — `poster_meta` stores release dates (for CDN TTL computation) and `available_ratings` records which rating sources have data for each movie (so cache keys can be reconstructed without external API calls on cache hits)
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
      POSTER_MEM_CACHE_MB: ${POSTER_MEM_CACHE_MB:-1024}
```

> `CACHE_DIR` can be omitted because no images are written to disk. The volume is still needed for the SQLite database (`DB_DIR`). `POSTER_MEM_CACHE_MB` is increased to 1024 because the in-memory cache is the only deduplication layer before Cloudflare — size it to fit your server's available RAM.

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
   - The SPA's `index.html` is served without a file extension on all non-API routes, so Cloudflare will not cache it by default. This is the desired behavior — it ensures users always load the latest HTML that references the current hashed assets.

6. **Browser TTL** (optional): Under **Caching > Configuration**, set **Browser Cache TTL** to **Respect Existing Headers** so the origin's `Cache-Control` headers are passed through to clients.

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
- **[Fanart.tv](https://fanart.tv/)** ([get API key](https://fanart.tv/get-an-api-key/)) — High-quality fan art, logos, and backdrops with language and textless support
- **[RPDB (Rating Poster Database)](https://ratingposterdb.com/)** — The original project that inspired OpenPosterDB's API design

### Rating Sources

- **[IMDb](https://www.imdb.com/)** — Internet Movie Database ratings
- **[Rotten Tomatoes](https://www.rottentomatoes.com/)** — Critics and audience scores
- **[Metacritic](https://www.metacritic.com/)** — Aggregated critic reviews
- **[Trakt](https://trakt.tv/)** — Community ratings and tracking
- **[Letterboxd](https://letterboxd.com/)** — Film community ratings
- **[MyAnimeList](https://myanimelist.net/)** — Anime and manga ratings
