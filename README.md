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

**Common parameters:**

- `id_type`: `imdb`, `tmdb`, `tvdb`
- `id_value`: e.g. `tt1234567`, `movie-123`, `series-456`
- `?fallback=true`: return a placeholder image instead of an error on failure
- RPDB-compatible — use `http://localhost:3000` as the base URL (drop-in replacement for `https://api.ratingposterdb.com`)

Management endpoints (auth, keys, settings) are under `/api/` and return JSON.

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
| `FANART_API_KEY` | — | [Fanart.tv](https://fanart.tv/get-an-api-key/) key (enables Fanart.tv as alternative poster source; required for logo and backdrop endpoints) |
| `CORS_ORIGIN` | — | Allowed origin for admin requests |
| `ADMIN_USERNAME` | — | Seed admin username on first run |
| `ADMIN_PASSWORD` | — | Seed admin password on first run |

## Cache Architecture

Images are cached in three layers: in-memory (moka), filesystem, and SQLite metadata. Cache keys encode all the settings that affect the rendered output so that different configurations produce separate cached files.

### Filesystem Layout

```
{CACHE_DIR}/
├── base/
│   ├── posters/          # Raw TMDB poster downloads (original filename)
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
{id_type}/{id_value}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}
```

**Fanart poster:**
```
{id_type}/{id_value}{variant}{ratings_suffix}{pos_suffix}{style_suffix}{label_suffix}{direction_suffix}
```

**Logo / Backdrop:**
```
{id_type}/{id_value}{kind_prefix}{variant}{ratings_suffix}{style_suffix}{label_suffix}
```

### Suffix Reference

| Suffix | Format | Example | Description |
|---|---|---|---|
| Ratings | `@{keys}` | `@mal,imdb,lb` | Ordered rating sources (truncated to limit) |
| Position | `.p{pos}` | `.pbc`, `.pl` | Poster badge position (`bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br`) |
| Badge style | `.s{style}` | `.sh`, `.sv` | `h` = horizontal, `v` = vertical |
| Label style | `.l{style}` | `.lt`, `.li` | `t` = text labels, `i` = icon labels |
| Badge direction | `.d{dir}` | `.dh`, `.dv` | `h` = horizontal, `v` = vertical (resolved from `d` = default) |

### Image Kind Prefixes

Logos and backdrops include a kind prefix in their cache keys to distinguish them from posters:

| Kind | Prefix |
|---|---|
| Poster | *(none)* |
| Logo | `:l` |
| Backdrop | `:b` |

### Fanart Variant Markers

When the poster source is fanart.tv, the cache key includes a variant marker indicating which fanart tier was used:

| Variant | Marker | Description |
|---|---|---|
| Textless | `:f:tl` | Fanart image with no text overlay |
| Language | `:f:{lang}` | Fanart image matching language (e.g. `:f:en`) |
| Negative (textless) | `:f:tl:neg` | No textless image available (stored in negative cache) |
| Negative (language) | `:f:{lang}:neg` | No language image available |

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
| `badge_style` | `h`, `v` | Horizontal, Vertical |
| `label_style` | `t`, `i` | Text, Icon |
| `badge_direction` | `d`, `h`, `v` | Default (auto-resolved by position), Horizontal, Vertical |
| `poster_position` | `bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br` | Bottom-center, Top-center, Left, Right, corners |

### Example Cache Keys

```
# TMDB poster, 3 ratings (mal,imdb,lb), bottom-center, horizontal badges, icon labels, horizontal direction
imdb/tt0111161@mal,imdb,lb.pbc.sh.li.dh

# Fanart textless poster
imdb/tt0111161:f:tl@mal,imdb,lb.pbc.sh.li.dh

# Logo with 3 ratings, horizontal badges, text labels
imdb/tt0111161:l:f:en@mal,imdb,lb.sh.lt

# Backdrop with vertical badges, icon labels
imdb/tt0111161:b@mal,imdb,lb.sv.li
```

### Staleness and Background Refresh

Cache entries are checked for staleness based on the film's release date:
- **Unreleased / unknown**: uses `RATINGS_STALE_SECS` (default 24h)
- **Recent films**: linearly increasing stale time from `RATINGS_STALE_SECS` to `RATINGS_MAX_AGE_SECS`
- **Old films** (age > `RATINGS_MAX_AGE_SECS`): never stale (ratings are stable)

When a stale entry is served, a background refresh is spawned to regenerate it without blocking the response. Request coalescing ensures concurrent requests for the same image share a single generation task.
