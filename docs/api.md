# API Reference

OpenPosterDB is a drop-in replacement for [RPDB](https://ratingposterdb.com) — use `http://localhost:3000` (or your instance URL) as the base URL in place of `https://api.ratingposterdb.com`.

- [Endpoints](#endpoints)
- [Common parameters](#common-parameters)
- [Image sizes](#image-sizes)

## Endpoints

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

### Key validation

```
GET /{api_key}/isValid
```

- Returns `200 OK` if the API key is valid, `401 Unauthorized` otherwise
- Compatible with RPDB integrations that validate keys before use

Management endpoints (auth, keys, settings) are under `/api/` and return JSON.

## Common parameters

- `id_type`: `imdb`, `tmdb`, `tvdb`
- `id_value`: e.g. `tt1234567`, `movie-123`, `series-456`, `episode-1396-S1E1`. Episode IMDb IDs (e.g. `tt0959621`), TVDB episode IDs, and series-level IDs with season/episode (e.g. `episode-tt14786934-S1E1`, `episode-81189-S3E5`) are also supported
- `?fallback=true`: accepted for RPDB plugin compatibility but ignored as OPDB falls back to TMDB by default
- `?lang={code}`: override the image language for this request (e.g. `?lang=de` for German, `?lang=pt-BR` for Brazilian Portuguese). Supports regional variants — when a region-specific image exists (e.g. `pt-BR`), it is preferred; otherwise falls back to the base language (`pt`), then English. Applies to posters and logos. Backdrops are language-agnostic and ignore this parameter
- `?imageSize={size}`: control output image dimensions. Available sizes vary by image type (see [Image sizes](#image-sizes))
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

Old RPDB parameter names `?poster_source=` and `?fanart_textless=` are accepted as aliases.

**Scope notes:** `textless`, `split`, and `fit` are poster-only. `blur` is episode-only. `edge_inset_x`/`edge_inset_y` are backdrop-only. `badge_direction` and `position` are silently ignored on logo endpoints. For shared parameters (`ratings_limit`, `badge_style`, `label_style`, `badge_size`, `badge_shape`, `badge_background`, `image_source`), the override is applied to the correct image-type-specific setting (e.g. `?badge_style=h` on the poster endpoint sets `poster_badge_style`, on the logo endpoint sets `logo_badge_style`).

## Image sizes

The `?imageSize=` parameter controls the output dimensions. When omitted, `medium` is used as the default. All badge elements scale proportionally with the image.

**Poster sizes:**

| Size | Dimensions |
|---|---|
| `medium` *(default)* | 580 × 859 |
| `large` | 1280 × 1896 |
| `very-large` / `verylarge` | 2000 × 2962 |

Heights are representative for a standard ~2:3 source under the default `native` fit, which preserves the source aspect ratio (so the exact height varies per poster). The `cover`, `pad`, and `blur` fits instead produce an exact 2:3 frame (580 × 870, 1280 × 1920, 2000 × 3000).

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
