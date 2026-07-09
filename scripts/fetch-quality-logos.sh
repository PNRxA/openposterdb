#!/usr/bin/env bash
#
# fetch-quality-logos.sh — download the quality-tier logo set used by the
# quality overlay badge (?quality=...&quality_style=logo) into
# api/assets/icons/quality/.
#
# Source: Wikimedia Commons (PNG thumbnails rasterized from the uploaded SVGs).
# These are brand/trademark logos used here as nominative quality indicators;
# they are rendered on a white plate so logos of any colour stay legible. Tiers
# without a bundled logo fall back to a text badge at render time.
#
# File name -> Wikimedia Commons source title:
#   4k.png    <- File:Ultra HD Blu-ray (logo).svg   (4K / UHD)
#   1080p.png <- File:Full HD 1080 logo (Sony).svg  (1080p / Full HD)
#   720p.png  <- File:HD ready logo.svg             (720p / HD Ready)
#   hdr.png   <- File:HDR10+ Logo.svg               (HDR)
#   dv.png    <- File:Dolby Vision (logo).svg       (Dolby Vision)
#
# Re-run to refresh the committed assets.

set -euo pipefail

WIDTH=400  # rasterize at 400px wide; the renderer scales down to badge height.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$SCRIPT_DIR/../api/assets/icons/quality"
mkdir -p "$DEST"

# name|Wikimedia Commons file title
ENTRIES=(
    "4k|File:Ultra HD Blu-ray (logo).svg"
    "1080p|File:Full HD 1080 logo (Sony).svg"
    "720p|File:HD ready logo.svg"
    "hdr|File:HDR10+ Logo.svg"
    "dv|File:Dolby Vision (logo).svg"
)

urlencode() { python3 -c "import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1]))" "$1"; }

thumburl() {
    local title="$1"
    curl -fsS --max-time 30 \
        "https://commons.wikimedia.org/w/api.php?action=query&format=json&prop=imageinfo&iiprop=url&iiurlwidth=$WIDTH&titles=$(urlencode "$title")" \
        | python3 -c "import sys,json;[print(v['imageinfo'][0].get('thumburl','')) if 'imageinfo' in v else print('') for v in json.load(sys.stdin)['query']['pages'].values()]"
}

echo ">> Downloading ${#ENTRIES[@]} quality logos (${WIDTH}px) -> $DEST"
for entry in "${ENTRIES[@]}"; do
    name="${entry%%|*}"
    title="${entry#*|}"
    url="$(thumburl "$title")"
    if [ -z "$url" ]; then
        echo >&2 "!! could not resolve thumb URL for $title"
        exit 1
    fi
    if curl -fsS --max-time 30 -o "$DEST/$name.png" "$url"; then
        printf '   %-6s <- %s\n' "$name" "$title"
    else
        echo >&2 "!! failed to fetch $name from $url"
        exit 1
    fi
done
echo ">> Done. $(ls "$DEST" | wc -l | tr -d ' ') logos in $DEST"
