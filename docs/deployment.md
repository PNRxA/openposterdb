# Deploying to the Public Internet

If you plan to expose OpenPosterDB to external users, put it behind a reverse proxy with TLS and (optionally) a CDN. The sections below cover using Caddy as the reverse proxy and Cloudflare as the CDN.

- [Reverse proxy with Caddy](#reverse-proxy-with-caddy)
- [Deploying behind Cloudflare](#cloudflare)

## Reverse proxy with Caddy

The repository includes a [`Caddyfile.example`](../Caddyfile.example). Copy it, replace the domain, and add a Caddy service to your compose file:

```yaml
services:
  caddy:
    image: caddy:2
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy-data:/data
    restart: unless-stopped

  openposterdb:
    image: pnrxa/openposterdb:latest
    environment:
      # refer to docker-compose.yml
    volumes:
      - openposterdb-data:/data
    restart: unless-stopped

volumes:
  caddy-data:
  openposterdb-data:
```

Caddy automatically provisions TLS certificates via Let's Encrypt. No extra configuration is needed — just point your DNS A record to the server's IP.

## Cloudflare

When Cloudflare sits in front of your origin, you can enable two environment flags that significantly reduce origin load:

```env
ENABLE_CDN_REDIRECTS=true
EXTERNAL_CACHE_ONLY=true
```

- **`ENABLE_CDN_REDIRECTS`** makes poster requests redirect to content-addressed `/c/` URLs so Cloudflare deduplicates cache entries across users with identical settings (see [Architecture → CDN caching](architecture.md#cdn-caching))
- **`EXTERNAL_CACHE_ONLY`** skips image file writes to disk, relying on Cloudflare's edge cache for long-term storage and the in-memory cache for short-term deduplication. SQLite metadata (release dates, available rating sources) is still written so cache keys and CDN TTLs can be computed without external API calls

Use the same Caddy + OpenPosterDB compose setup from the [reverse proxy section](#reverse-proxy-with-caddy), adding the two CDN flags and bumping the in-memory cache. The key changes to the `openposterdb` environment:

```yaml
      # Add these to your existing environment block
      ENABLE_CDN_REDIRECTS: "true"
      EXTERNAL_CACHE_ONLY: "true"
      IMAGE_MEM_CACHE_MB: ${IMAGE_MEM_CACHE_MB:-1024}
```

> `CACHE_DIR` can be omitted because no images are written to disk. The volume is still needed for the SQLite database (`DB_DIR`). `IMAGE_MEM_CACHE_MB` is increased to 1024 because the in-memory cache is the only deduplication layer before Cloudflare — size it to fit your server's available RAM.

### Cloudflare configuration

1. **DNS**: Add an A record pointing to your server's IP with the orange cloud (Proxied) enabled.

2. **SSL/TLS**: Go to **SSL/TLS > Overview** and set the mode to **Full (strict)**. This encrypts traffic between Cloudflare and your origin. Caddy's auto-TLS handles the origin certificate, or you can use a [Cloudflare Origin CA certificate](https://developers.cloudflare.com/ssl/origin-configuration/origin-ca/).

3. **Cache Rules**: Cloudflare already caches `.jpg` and `.png` responses by default. The origin sets a dynamic `Cache-Control` header based on film age (1 day for new releases, up to 1 year for older films). To ensure Cloudflare respects this, add a cache rule:
   - Go to **Caching > Cache Rules** and create a rule:
     - **When**: URI Path starts with `/c/`
     - **Then**: **Eligible for cache**, set **Edge TTL** to **Respect origin**
   - This ensures new releases get short edge TTLs (so updated ratings propagate quickly) while old films stay cached for up to a year.

4. **Tiered Cache**: Go to **Caching > Tiered Cache** and enable **Smart Tiered Caching**. This reduces origin hits by allowing Cloudflare's upper-tier data centers to serve cache hits to lower-tier ones.

5. **Static Assets**: The web UI's static assets (JS, CSS, fonts, images) are built by Vite with content hashes in their filenames (e.g. `assets/index-abc123.js`), making them safe to cache aggressively. Add a second cache rule:
   - **When**: URI Path starts with `/assets/`
   - **Then**: **Eligible for cache**, set **Edge TTL** to 1 year, **Browser TTL** to 1 year
   - These files are immutable — when the app is redeployed, Vite generates new filenames, so stale cache entries are never served.
   - The SPA's `index.html` is served without a file extension on all non-API routes, so Cloudflare will not cache it by default without a rule (see below).

6. **Web Console (origin-down resilience)**: The origin sets `Cache-Control: public, max-age=60, stale-while-revalidate=3600, stale-if-error=86400` on all SPA responses (HTML and static files). Add a cache rule to let Cloudflare respect these headers for the HTML shell:
   - **When**: Hostname equals the domain **AND** URI Path does not start with `/api/` **AND** URI Path does not start with `/c/` **AND** URI Path does not start with `/assets/`
   - **Then**: **Eligible for cache**, **Edge TTL** = **Respect origin**
   - This keeps `index.html` fresh (60 s browser TTL) during normal operation, but allows Cloudflare to serve a stale copy for up to 24 hours (`stale-if-error`) when the origin is unreachable — making the full web console available from cache during outages. Hashed `/assets/` files are already cached by the rule above.

7. **Browser TTL** (optional): Under **Caching > Configuration**, set **Browser Cache TTL** to **Respect Existing Headers** so the origin's `Cache-Control` headers are passed through to clients.

### How the CDN flow works

```
Client → Cloudflare edge
  → /{api_key}/imdb/poster-default/tt1234567.jpg
  → Origin validates API key, returns 302 → /c/{hash}/imdb/poster-default/tt1234567.jpg
  → Cloudflare follows redirect internally (or client follows it)
  → /c/{hash}/... → Cache HIT at edge (served from Cloudflare)
     or Cache MISS → Origin renders image → Cloudflare caches it → Response
```

After the first request, all users with the same settings get the cached image directly from Cloudflare's edge — the origin is not hit.
