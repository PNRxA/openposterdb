# Architecture

Internal reference for how OpenPosterDB caches and renders images. You don't need any of this to self-host — it's here for contributors and anyone debugging cache behaviour.

- [Tech stack](#tech-stack)
- [Cache architecture](#cache-architecture)
- [Clearing the cache](#clearing-the-cache)

## Tech stack

- **API**: Rust, Axum, SeaORM + SQLite, image/imageproc for rendering
- **Web**: Vue 3, TypeScript, Tailwind CSS, Vite

## Cache architecture

Images are cached in three layers: in-memory (moka), filesystem, and SQLite metadata. Cache keys encode all the settings that affect the rendered output so that different configurations produce separate cached files.

### Filesystem layout

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

### Cache key format

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

### Suffix reference

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

### Image kind prefixes

Logos, backdrops, and episodes include a kind prefix in their cache keys to distinguish them from posters:

| Kind | Prefix |
|---|---|
| Poster | *(none)* |
| Logo | `_l` |
| Backdrop | `_b` |
| Episode | `_e` |

### Source variant markers

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

### Database values

The `image_meta` table tracks metadata for cached images:

| Field | Short Value | Meaning |
|---|---|---|
| `image_type` | `p` | Poster |
| `image_type` | `l` | Logo |
| `image_type` | `b` | Backdrop |
| `image_type` | `e` | Episode |

### Settings short values

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

### Example cache keys

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

### Cross-ID cache

When a poster is generated via one ID type (e.g. IMDB), the rendered image is also written to the filesystem cache under all resolved alternate IDs (TMDB, TVDB). This avoids redundant image generation when the same content is requested via different ID types.

- Alternate IDs are determined from the moka-cached `ResolvedId` (no extra API calls)
- Writes are best-effort and parallelized — errors are logged but not propagated
- Only the filesystem cache and DB metadata are populated; the in-memory cache is not — alternate keys get promoted to memory on their first actual request
- Applies to all image types: posters, logos, and backdrops

### Staleness and background refresh

Cache entries are checked for staleness based on the film's release date:
- **Unreleased / unknown**: uses `RATINGS_STALE_SECS` (default 24h)
- **Recent films**: linearly increasing stale time from `RATINGS_STALE_SECS` to `RATINGS_MAX_AGE_SECS`
- **Old films** (age > `RATINGS_MAX_AGE_SECS`): never stale (ratings are stable)

When a stale entry is served, a background refresh is spawned to regenerate it without blocking the response. Request coalescing ensures concurrent requests for the same image share a single generation task.

### CDN caching

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

### External cache only

When `EXTERNAL_CACHE_ONLY=true`, the server skips image file writes to disk (rendered posters and base source images from TMDB/Fanart.tv). This is useful when deployed behind a CDN like Cloudflare that caches responses at the edge.

- The in-memory (moka) cache still handles short-term request deduplication
- Request coalescing still prevents duplicate generation for concurrent requests
- The cache directory is not created on startup
- Filesystem reads naturally return misses (no files on disk), so every request either hits the in-memory cache or regenerates the image
- Best used together with `ENABLE_CDN_REDIRECTS=true` so the CDN absorbs the vast majority of traffic
- SQLite metadata is **always** written, even with this flag — `image_meta` stores release dates (for CDN TTL computation) and `available_ratings` records which rating sources have data for each movie (so cache keys can be reconstructed without external API calls on cache hits)
- The Docker volume is still required for the SQLite database (`DB_DIR`), even when image caching is fully external

## Clearing the cache

The admin panel can purge cached images without touching the database volume or restarting the container — useful when a poster rendered from a bad source image, when global render settings changed and left orphaned variants behind, or for general cache hygiene.

- **Clear everything** — the **Clear cache** button on the dashboard (and on the **Settings** page) wipes all rendered images, raw downloads, and settings-preview thumbnails on disk, every `image_meta` / `available_ratings` row, and every in-memory image cache (including the settings-preview cache and the upstream TMDB/Fanart.tv image-list and ratings caches). Images regenerate from scratch on the next request, so the first load of each title afterwards is slower. This is the path that guarantees a fully clean re-fetch. The on-disk wipe is **instant regardless of cache size** — the cache directories are atomically renamed aside and the (potentially slow) recursive delete runs in the background — so the request returns immediately even with hundreds of thousands of files, and an interrupted delete is swept on the next startup.
- **Clear one image type** — the **Clear posters / logos / backdrops / episodes** button at the top of each list view removes all cached images of just that kind (its rendered directory + `image_meta` rows), leaving the other kinds and the shared `available_ratings` index untouched. Like clear-all, the on-disk wipe is staged aside and removed in the background.
- **Purge one title, or one variant** — the trash button on a row in the poster/logo/backdrop/episode lists opens a dialog with two choices:
  - **Entire title** removes *every* cached variant of that title for that image kind. One title maps to many cache entries (the key encodes ratings, position, style, size, language, …), so this prefix-matches the title id rather than deleting a single key.
  - **This variant** removes only the single rendered entry the row represents (one exact cache key), leaving the title's other variants and its shared `available_ratings` index untouched.

Each purge clears the relevant layers consistently: the in-memory render caches, the rendered files on disk, and the SQLite metadata (`image_meta` plus the title's `available_ratings` index, so the next request re-resolves its sources).

A per-title purge is scoped to **rendered output**. A couple of things it deliberately does not reach, because they self-heal:

- **Upstream source caches** (the TMDB/Fanart.tv image lists and aggregated ratings) are keyed by the resolved TMDB id and expire on their own short TTL (~30–60 min). A re-render right after a per-title purge may briefly reuse them, so for an immediate clean re-fetch of changed upstream art/ratings use **Clear cache** instead.
- **Cross-ID copies** — the same title cached under a different id form (e.g. an `imdb` request and a `tmdb` request resolving to the same movie) live under a different `id_type`. A purge targets the id form you pass; the alternate copy regenerates or expires on its own.

The matching API endpoints (behind the admin auth middleware) are `POST /api/admin/cache/purge` (clear everything), `DELETE /api/admin/{posters,logos,backdrops,episodes}` (clear one kind), and `DELETE /api/admin/{posters,logos,backdrops,episodes}/{id_type}/{id_value}` (purge one title — or a single variant with `?scope=variant`, in which case `{id_value}` is the full cache value rather than the bare title id).

Under `EXTERNAL_CACHE_ONLY`, there are no files on disk to remove and the CDN's cached copies cannot be purged from here, so a purge clears only the in-memory caches and SQLite metadata. The admin UI surfaces this as a partial purge.
