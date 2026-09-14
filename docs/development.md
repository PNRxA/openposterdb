# Building from Source

Most self-hosters should use Docker (see the [README](../README.md)). These instructions are for local development or running without Docker.

## Requirements

- Rust toolchain
- Node.js 20.19+ (for the admin UI)
- A [TMDB API key](https://www.themoviedb.org/settings/api)
- At least one ratings source: [MDBList API key](https://mdblist.com/preferences/) (preferred — covers all 9 rating sources), [OMDb API key](https://www.omdbapi.com/apikey.aspx), or [Trakt Client ID](https://trakt.tv/oauth/applications)
- Optional: [Fanart.tv API key](https://fanart.tv/get-an-api-key/) (enables Fanart.tv as an alternative or preferred image source)

## API

```bash
cd api
cp .env.example .env
# Edit .env — at minimum set TMDB_API_KEY, MDBLIST_API_KEY (or OMDB), and JWT_SECRET
cargo run --release
```

See [Configuration](configuration.md) for the full list of environment variables.

## Web UI

```bash
cd web
npm install
npm run dev        # development
npm run build      # production
```

The web UI will be available at `http://localhost:3000`. On first visit you'll be prompted to create an admin account.

If you access the UI over plain HTTP (no reverse proxy with TLS), add `COOKIE_SECURE=false` to your `.env` — otherwise the browser will silently drop auth cookies and login will appear broken.

## Tech stack

- **API**: Rust, Axum, SeaORM + SQLite, image/imageproc for rendering
- **Web**: Vue 3, TypeScript, Tailwind CSS, Vite

See [Architecture](architecture.md) for how caching and rendering work internally.
