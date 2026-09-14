# Connecting a Media Server

OpenPosterDB serves rating-badged posters, backdrops, logos, and episode stills to any client that can fetch images by URL. Replace `{base_url}` with your instance URL (e.g. `http://localhost:3000` or `https://openposterdb.com`) and `{api_key}` with your API key throughout.

- [aiometadata](#aiometadata)
- [Jellyfin](#jellyfin)
- [Plex](#plex)

## aiometadata

To use OpenPosterDB with [aiometadata](https://github.com/cedya77/aiometadata), set the following URL templates in your aiometadata configuration. `{type}` is `movie` or `series` depending on the media type.

| Image type | URL template |
|---|---|
| Poster | `{base_url}/{api_key}/imdb/poster-default/{imdb_id}.jpg` |
| Background | `{base_url}/{api_key}/tmdb/backdrop-default/{type}-{tmdb_id}.jpg?imageSize=large` |
| Logo | `{base_url}/{api_key}/tmdb/logo-default/{type}-{tmdb_id}.png` |
| Episode | `{base_url}/{api_key}/imdb/episode-default/episode-{imdb_id}-S{season}E{episode}.jpg` |

TMDB and TVDB IDs also work for episodes:

| ID type | Episode URL template |
|---|---|
| TMDB | `{base_url}/{api_key}/tmdb/episode-default/episode-{tmdb_id}-S{season}E{episode}.jpg` |
| TVDB | `{base_url}/{api_key}/tvdb/episode-default/episode-{tvdb_id}-S{season}E{episode}.jpg` |

The episode endpoint accepts a series-level ID combined with season/episode numbers — no episode-specific ID is needed.

## Jellyfin

OpenPosterDB has a dedicated [Jellyfin plugin](https://github.com/PNRxA/jellyfin-plugin-openposterdb) — a separate, open-source remote image provider. It fetches posters, backdrops, logos and episode stills (with rating badges) from your self-hosted instance, keyed off each item's IMDb / TMDB / TVDB id.

1. In Jellyfin, go to **Dashboard → Plugins → Repositories** and add the manifest URL:
   ```
   https://raw.githubusercontent.com/PNRxA/jellyfin-plugin-openposterdb/main/manifest.json
   ```
2. Install "OpenPosterDB" from the **Catalog**, then restart Jellyfin.
3. Open the plugin's settings and set the **Base URL** and **API key** for your OpenPosterDB instance.

See the [plugin repo](https://github.com/PNRxA/jellyfin-plugin-openposterdb) for version and compatibility details.

## Plex

Plex deprecated its legacy plugin agents, so OpenPosterDB connects through a **Custom Metadata Provider** (Plex Media Server **1.43+**). A hosted provider runs at **`https://plex.openposterdb.com`** — nothing to install. It's open source at [openposterdb-plex-provider](https://github.com/PNRxA/openposterdb-plex-provider) if you'd rather self-host.

Add it as two providers — one for movies, one for TV — each with your OpenPosterDB API key in the URL:

| Library | Provider URL |
|---|---|
| Movies | `https://plex.openposterdb.com/{api_key}/movie` |
| TV | `https://plex.openposterdb.com/{api_key}/tv` |

1. In Plex, go to **Settings → Manage → Metadata Agents → Add Provider** and add the Movies URL above; add the TV URL as a second provider.
2. **Add Agent** — create an agent that pairs the OpenPosterDB provider with **Plex Movie** (and another with **Plex TV Series**) so Plex still supplies titles and metadata, then drag OpenPosterDB above the default art source.
3. Assign the agent to each library under **Library → Edit → Advanced → Agent**, then **Refresh Metadata**.

To pin a poster style, append OpenPosterDB [options](api.md) to the mount segment, e.g. `https://plex.openposterdb.com/{api_key}/movie~badge_style=v&ratings_limit=5`.

> [!NOTE]
> Plex's Custom Metadata Provider API is in beta and requires PMS 1.43+.
