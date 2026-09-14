# Configuration

OpenPosterDB is configured entirely through environment variables. When using Docker Compose, set them in a `.env` file at the project root (copy `api/.env.example` to get started); when using `docker run`, pass them with `-e`.

## API keys

At minimum you need a **TMDB key** (for artwork), a **JWT secret** (for auth), and **one ratings source**. MDBList is recommended because a single key covers all nine rating sources.

| Variable | Default | Description |
|---|---|---|
| `TMDB_API_KEY` | *required* | [TMDB](https://www.themoviedb.org/settings/api) API v3 key |
| `JWT_SECRET` | *required* | 32-byte hex string (`openssl rand -hex 32`) |
| `MDBLIST_API_KEY` | — | [MDBList](https://mdblist.com/preferences/) key — preferred, covers all 9 rating sources (IMDb, RT Critics, RT Audience, Metacritic, Trakt, Letterboxd, MAL, MDBList score, Roger Ebert) |
| `OMDB_API_KEY` | — | [OMDb](https://www.omdbapi.com/apikey.aspx) key (IMDb, RT Critics, Metacritic only). Also required for IMDB episode ratings |
| `TRAKT_CLIENT_ID` | — | [Trakt](https://trakt.tv/oauth/applications) Client ID — Trakt community ratings for movies, shows, and episodes |
| `FANART_API_KEY` | — | [Fanart.tv](https://fanart.tv/get-an-api-key/) key — enables Fanart.tv as an alternative or preferred image source for posters, logos, and backdrops |

## Server & storage

| Variable | Default | Description |
|---|---|---|
| `LISTEN_ADDR` | `0.0.0.0:3000` | Server bind address |
| `CACHE_DIR` | `./cache` | Poster and metadata cache directory |
| `DB_DIR` | `./db` | SQLite database directory |
| `IMAGE_QUALITY` | `85` | JPEG output quality (1-100) |
| `IMAGE_MEM_CACHE_MB` | `512` | In-memory cache size in MB |
| `RENDER_CONCURRENCY` | `CPUs × 2` | Max concurrent image render tasks |
| `CROSS_ID_CONCURRENCY` | `CPUs` | Max concurrent cross-ID cache write tasks |

## Auth & access

| Variable | Default | Description |
|---|---|---|
| `COOKIE_SECURE` | `true` | HTTPS-only cookies. Set to `false` if you access the admin UI over plain HTTP (no TLS) — otherwise the browser drops auth cookies and login appears broken |
| `CORS_ORIGIN` | — | Allowed origin for admin requests |
| `ADMIN_USERNAME` | — | Seed admin username on first run |
| `ADMIN_PASSWORD` | — | Seed admin password on first run |
| `FREE_KEY_ENABLED` | — | Force-enable (`true`) or force-disable (`false`) the free API key, overriding the admin UI toggle. When set, the UI toggle is locked. Omit to let admins control it from the settings page |
| `DISABLE_PUBLIC_PAGES` | `false` | Hide the landing page, docs, legal, and all unauthenticated routes, redirecting visitors to the login page. All pages remain accessible to authenticated users. Useful for private instances |

## Caching & refresh

| Variable | Default | Description |
|---|---|---|
| `RATINGS_STALE_SECS` | `86400` | Min ratings cache lifetime |
| `RATINGS_MAX_AGE_SECS` | `31536000` | Film age after which ratings stop refreshing |
| `IMAGE_STALE_SECS` | `0` | Base image cache lifetime (0 = never re-fetch) |
| `ENABLE_CDN_REDIRECTS` | `false` | Enable content-addressed CDN redirects (see [Deployment](deployment.md#cloudflare)) |
| `EXTERNAL_CACHE_ONLY` | `false` | Skip image file writes to disk; rely on a CDN for caching. SQLite metadata is still written (see [Architecture](architecture.md#external-cache-only)) |

## Logging

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `warn` | Log level filter — levels: `error`, `warn`, `info`, `debug`, `trace`. Supports comma-separated per-module overrides. Relevant modules: `openposterdb_api` (app), `tower_http` (HTTP tracing), `sea_orm` / `sqlx` (database), `reqwest` / `hyper` (HTTP client/server). Example: `warn,openposterdb_api=info,tower_http=debug` |
