# Scripts

Utility scripts for developing, testing, releasing, and seeding OpenPosterDB.

## dev-worktree.sh

Prepares a freshly created git worktree for local development and runs it. A new
worktree is missing all the gitignored bits the app needs (`api/.env`, `api/db`,
`api/cache`, `web/node_modules`); this gets one runnable in a single command.

```bash
./scripts/dev-worktree.sh              # set up (if needed) then run API + web
./scripts/dev-worktree.sh --fresh      # also wipe api/db + api/cache first
./scripts/dev-worktree.sh --release    # build/run the API in release mode
./scripts/dev-worktree.sh --setup-only # copy env + install deps, don't run
```

Run it from inside the worktree you want to set up (any subdirectory is fine).

### What it does

1. Copies `api/.env` from the **main** worktree if the current worktree lacks one (never overwrites an existing one).
2. With `--fresh`, deletes the current worktree's `api/db` and `api/cache` for a clean slate before running. In the **main** worktree this would wipe your primary data, so it asks for confirmation first (or pass `--yes`); in a linked worktree it wipes without asking.
3. Installs web dependencies (`npm ci`, falling back to `npm install`) if `web/node_modules` is missing.
4. Runs the Rust API (`cargo run` in `api/`, on `http://localhost:3000`) and the Vite dev server (`npm run dev` in `web/`, usually `http://localhost:5173`) together. Press Ctrl+C — or let either process exit — to stop both.

> The dev UI is served by Vite over plain HTTP at `http://localhost:5173`. If login
> appears broken, add `COOKIE_SECURE=false` to `api/.env` (see the main README) —
> otherwise the browser drops the auth cookie.

---

## seed.sh

Warms the OpenPosterDB cache by requesting posters for titles from the IMDB dataset. Entries are processed newest-first, using `endYear` for series (if available) and `startYear` otherwise.

Requires `title.basics.tsv` in the scripts directory. Download it from <https://datasets.imdbws.com/title.basics.tsv.gz> and extract.

```bash
./scripts/seed.sh <BASE_URL> [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-n, --limit NUM` | Total entries to seed (0 = unlimited) | `100` |
| `-N, --limit-per-type NUM` | Cap each type independently (e.g. 100k movies + 100k series) | none |
| `-t, --type TYPE` | `movie`, `tv`, or `both` | `both` |
| `-g, --genres GENRES` | Comma-separated genres to include (e.g. `"Action,Horror"`) | all |
| `-f, --year-from YEAR` | Minimum year (inclusive) | none |
| `-y, --year-to YEAR` | Maximum year (inclusive) | none |
| `-k, --key KEY` | API key to use | `t0-free-rpdb` |
| `-a, --assets ASSETS` | `poster`, `logo`, `backdrop`, or `all` | `poster` |
| `-d, --dry-run` | Print matching titles without making requests | off |

### Examples

```bash
# Seed the 100 newest titles (default)
./scripts/seed.sh http://localhost:3000

# Seed 500 movies from 2000 onwards
./scripts/seed.sh http://localhost:3000 -n 500 -t movie -f 2000

# Seed all horror and thriller titles, including logos and backdrops
./scripts/seed.sh http://localhost:3000 -n 0 -g "Horror,Thriller" -a all

# Preview what TV series from 2015-2023 would be seeded
./scripts/seed.sh http://localhost:3000 -t tv -f 2015 -y 2023 -d

# Seed 100k movies + 100k series in one run
./scripts/seed.sh http://localhost:3000 -N 100000


```

### Data files

| File | Description |
|------|-------------|
| `title.basics.tsv` | Extracted IMDB dataset used by the seed script. Not committed to git. |
| `imdb_ids.txt` | Plain list of all IMDB IDs (one per line, sorted). |

---

## test.sh

Runs the full test suite: backend (Rust), frontend unit tests (Vitest), and end-to-end tests (Playwright).

```bash
./scripts/test.sh
```

### What it does

1. Runs `cargo test` in `api/`
2. Runs `npx vitest run` in `web/`
3. Builds a container image with the `test-support` feature flag
4. Starts the container on port `3333`, loading API keys from `api/.env`
5. Waits for the backend to become healthy (up to 60s)
6. Runs Playwright E2E tests (`setup`, `settings`, `chromium`, `live` projects)
7. Tears down the container on exit

Requires either `podman` or `docker`.

---

## release.sh

Creates a new tagged release and pushes it to GitHub.

```bash
./scripts/release.sh <VERSION>
./scripts/release.sh repair
```

### Create a release

```bash
./scripts/release.sh 1.0.0
```

1. Checks that the working tree is clean
2. Updates the version in `api/Cargo.toml` and `web/package.json`
3. Updates `Cargo.lock` and `package-lock.json`
4. Commits, pushes to `main`, and creates a GitHub release with auto-generated notes

The `v` prefix is added automatically — pass `1.0.0`, not `v1.0.0`.

### Repair a release

```bash
./scripts/release.sh repair
```

Re-triggers the release workflow for the most recent tag by deleting and recreating the GitHub release. Useful when CI failed on the initial release.

---

## regenerate-examples.sh

Fetches example poster, logo, and backdrop images for a set of classic films and saves them to `web/public/examples/`. These are used in the web UI.

```bash
./scripts/regenerate-examples.sh [BASE_URL]
```

Defaults to `http://localhost:3000`. Uses the free API key. Fetches assets for:

- Nosferatu (`tt0013442`)
- Metropolis (`tt0017136`)
- The Cabinet of Dr. Caligari (`tt0010323`)
- The Phantom of the Opera (`tt0016220`)
- A Trip to the Moon (`tt0000417`)
- Safety Last! (`tt0014429`)
- The General (`tt0017925`)

Logos and backdrops that aren't available are silently skipped.

---

## fetch-flags.sh

Downloads the language-flag icon set used by the language overlay badge
(`?lang_icon=flag`) into `api/assets/icons/flags/`, keyed by ISO 3166-1 country
code. Source: [flagcdn.com](https://flagcdn.com) (public-domain flags derived
from [flag-icons](https://github.com/lipis/flag-icons)).

```bash
./scripts/fetch-flags.sh
```

The API maps a title's TMDB `original_language` to a representative country (see
`flag_country_for_lang` in `api/src/image/icons.rs`); languages without a mapped
flag fall back to a text badge. Edit the country list in the script to add flags,
then re-run to refresh the committed assets.

---

## fetch-quality-logos.sh

Downloads the quality-tier logo set used by the quality overlay badge
(`?quality=...&quality_style=logo`) into `api/assets/icons/quality/`. Source:
Wikimedia Commons (PNG thumbnails rasterized from the uploaded SVGs). Logos are
rendered on a white plate so any logo colour stays legible.

```bash
./scripts/fetch-quality-logos.sh
```

Tiers without a bundled logo fall back to a text badge. Requires `python3` (used
to resolve Wikimedia thumbnail URLs).
