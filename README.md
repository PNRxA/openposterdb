> [!NOTE]
> This project is developed with the assistance of AI code generation tools. AI-generated code is reviewed and tested before being merged, but if you encounter any issues, please feel free to open an issue or submit a pull request.

# OpenPosterDB

A self-hosted, drop-in replacement for [RPDB (Rating Poster Database)](https://ratingposterdb.com). Generates movie and TV show posters, logos, and backdrops with rating badges from multiple sources overlaid on them. Fetches art from TMDB (or optionally [Fanart.tv](https://fanart.tv)), aggregates ratings from IMDb, Rotten Tomatoes, Metacritic, Trakt, Letterboxd, MyAnimeList, and composites color-coded badges onto the image.

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
