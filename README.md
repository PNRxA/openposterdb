# OpenPosterDB

Self-hosted API that generates movie and TV show posters with rating badges from multiple sources overlaid on them. Fetches poster art from TMDB, aggregates ratings from IMDb, Rotten Tomatoes, Metacritic, Trakt, Letterboxd, MyAnimeList, and composites color-coded badges onto the image.

## Features

- **Multi-source ratings** — Aggregates from OMDb (IMDb, RT, Metacritic) and MDBList (Trakt, Letterboxd, MAL)
- **ID resolution** — Accepts IMDb, TMDB, or TVDB IDs
- **Multi-layer caching** — In-memory (moka), filesystem, and SQLite metadata with background refresh and request coalescing
- **Admin UI** — Vue 3 web panel for API key management
- **Auth** — Argon2 password hashing, JWT access tokens, rotating refresh tokens, API key access for poster endpoints

## Tech Stack

- **API**: Rust, Axum, SeaORM + SQLite, image/imageproc for rendering
- **Web**: Vue 3, TypeScript, Tailwind CSS, Vite

## Quick Start

### Requirements

- Rust toolchain
- Node.js 20.19+ (for admin UI)
- A [TMDB API key](https://www.themoviedb.org/settings/api)
- At least one of: [OMDb API key](https://www.omdbapi.com/apikey.aspx), [MDBList API key](https://mdblist.com/preferences/)

### API

```bash
cd api

# Generate a JWT secret
JWT_SECRET=$(openssl rand -hex 32)

# Create .env
cat > .env << EOF
TMDB_API_KEY=your_key
OMDB_API_KEY=your_key       # and/or MDBLIST_API_KEY
JWT_SECRET=$JWT_SECRET
EOF

cargo run --release
```

### Web UI

```bash
cd web
npm install
npm run dev        # development
npm run build      # production
```

## Configuration

| Variable | Default | Description |
|---|---|---|
| `TMDB_API_KEY` | *required* | TMDB API v3 key |
| `JWT_SECRET` | *required* | 32-byte hex string (`openssl rand -hex 32`) |
| `OMDB_API_KEY` | — | OMDb key (IMDb, RT, Metacritic ratings) |
| `MDBLIST_API_KEY` | — | MDBList key (Trakt, Letterboxd, MAL ratings) |
| `LISTEN_ADDR` | `0.0.0.0:3000` | Server bind address |
| `CACHE_DIR` | `./cache` | Poster and metadata cache directory |
| `POSTER_QUALITY` | `85` | JPEG output quality (1-100) |
| `POSTER_MEM_CACHE_MB` | `512` | In-memory cache size in MB |
| `RATINGS_STALE_SECS` | `86400` | Min ratings cache lifetime |
| `RATINGS_MAX_AGE_SECS` | `31536000` | Film age after which ratings stop refreshing |
| `POSTER_STALE_SECS` | `0` | Base poster cache lifetime (0 = never re-fetch) |
| `COOKIE_SECURE` | `true` | HTTPS-only cookies |
| `CORS_ORIGIN` | — | Allowed origin for admin requests |
| `ADMIN_USERNAME` | — | Seed admin username on first run |
| `ADMIN_PASSWORD` | — | Seed admin password on first run |

## API Endpoints

### Poster

```
GET /{api_key}/{id_type}/poster-default/{id_value}.jpg
```

- `id_type`: `imdb`, `tmdb`, `tvdb`
- `id_value`: e.g. `tt1234567`, `movie-123`, `series-456`
- Returns JPEG with rating badges
