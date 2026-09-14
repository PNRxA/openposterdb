# OpenPosterDB (OPDB)

A self-hosted, drop-in replacement for [RPDB (Rating Poster Database)](https://ratingposterdb.com). It generates movie and TV posters, logos, backdrops, and episode stills with rating badges overlaid on them — pulling artwork from TMDB (or optionally [Fanart.tv](https://fanart.tv)) and aggregating ratings from IMDb, Rotten Tomatoes, Metacritic, Trakt, Letterboxd, MyAnimeList, MDBList, and Roger Ebert.

[Website](https://openposterdb.com) · [GitHub](https://github.com/pnrxa/openposterdb) · [Docker Hub](https://hub.docker.com/r/pnrxa/openposterdb) · [GitHub Packages](https://ghcr.io/pnrxa/openposterdb) · [**Documentation**](docs/README.md)

> [!NOTE]
> This project is developed with the assistance of AI code generation tools. AI-generated code is reviewed and tested before being merged. If you hit any issues, please open an issue or a pull request.

> [!WARNING]
> This project is in active development. Expect breaking changes to the database schema and cache key format until a stable release. After updating you may need to clear your cache to drop orphaned entries — use the admin dashboard's **Clear cache** button (no need to delete the database volume).

## Don't want to self-host?

A hosted version runs at **[openposterdb.com](https://openposterdb.com)** — no setup, just grab a free API key and point your media server at it.

## Self-hosting

OpenPosterDB runs as a single Docker container. You'll need a few free API keys first:

- **[TMDB](https://www.themoviedb.org/settings/api)** (required) — artwork
- **[MDBList](https://mdblist.com/preferences/)** (recommended) — one key covers all 9 rating sources
- A random **JWT secret** for auth — generate one with `openssl rand -hex 32`

### Docker Compose (recommended)

```bash
# Copy the example env to the project root and fill in your keys
cp api/.env.example .env
# Edit .env — at minimum set TMDB_API_KEY, MDBLIST_API_KEY (or OMDB_API_KEY), and JWT_SECRET

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

Then open **http://localhost:3000** and create your admin account on first visit.

> If you access the UI over plain HTTP (no TLS in front), add `-e COOKIE_SECURE=false` — otherwise the browser drops the auth cookie and login appears broken.

See **[Configuration](docs/configuration.md)** for the full list of environment variables (ratings sources, caching, logging, and more).

## Connect your media server

Grab an API key from the admin UI, then point your client at your instance:

- **[aiometadata](docs/media-servers.md#aiometadata)** — poster/backdrop/logo/episode URL templates
- **[Jellyfin](docs/media-servers.md#jellyfin)** — dedicated remote image provider plugin
- **[Plex](docs/media-servers.md#plex)** — Custom Metadata Provider (PMS 1.43+)

Any client that fetches images by URL works too — see the **[API Reference](docs/api.md)**.

## Going public

Exposing OpenPosterDB to external users? Put it behind a reverse proxy with TLS, and optionally a CDN. The **[Deployment guide](docs/deployment.md)** covers Caddy (auto-TLS) and Cloudflare (edge caching that shares one cache entry across users with identical settings).

## Features

- **Multi-source ratings** — Aggregates from MDBList (IMDb, RT Critics, RT Audience, Metacritic, Trakt, Letterboxd, MAL, the MDBList score, and Roger Ebert), optionally OMDb, and optionally Trakt directly (for episode-level ratings and as a standalone provider)
- **Multiple image sources** — TMDB as the primary source for posters, logos, and backdrops, with Fanart.tv as an optional fallback (or preferred source). Supports language selection and textless posters from both sources
- **Configurable per API key** — Override image source, language, and textless settings per key, or set global defaults
- **ID resolution** — Accepts IMDb, TMDB, or TVDB IDs
- **Multi-layer caching** — In-memory (moka), filesystem, and SQLite metadata with background refresh and request coalescing
- **Admin UI** — Vue 3 web panel for API key management, poster settings, global configuration, and cache purging
- **Auth** — Argon2 password hashing, JWT access tokens, rotating refresh tokens, API key access for poster endpoints

## Documentation

- **[Connecting a Media Server](docs/media-servers.md)** — aiometadata, Jellyfin, Plex
- **[Configuration](docs/configuration.md)** — all environment variables
- **[API Reference](docs/api.md)** — endpoints, parameters, image sizes
- **[Deployment](docs/deployment.md)** — reverse proxy and CDN
- **[Building from Source](docs/development.md)** — running without Docker
- **[Architecture](docs/architecture.md)** — caching internals for contributors

## Acknowledgments

OpenPosterDB is made possible by these third-party services and projects:

**Data & image providers** — [TMDB](https://www.themoviedb.org/) (movie/TV metadata and posters; this product uses the TMDB API but is not endorsed or certified by TMDB), [MDBList](https://mdblist.com/) (aggregated ratings), [OMDb](https://www.omdbapi.com/) (alternative ratings for IMDb, RT, Metacritic), [Fanart.tv](https://fanart.tv/) (fan art, logos, backdrops), [RPDB](https://ratingposterdb.com/) (the original project that inspired OPDB's API design), and [Simple Icons](https://simpleicons.org/) (brand SVG icons).

**Rating sources** — [IMDb](https://www.imdb.com/), [Rotten Tomatoes](https://www.rottentomatoes.com/), [Metacritic](https://www.metacritic.com/), [Trakt](https://trakt.tv/), [Letterboxd](https://letterboxd.com/), and [MyAnimeList](https://myanimelist.net/).

## License

See [LICENSE](LICENSE).
