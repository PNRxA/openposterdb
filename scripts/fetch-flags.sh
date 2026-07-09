#!/usr/bin/env bash
#
# fetch-flags.sh — download the language-flag icon set used by the language
# overlay badge (?lang_icon=flag) into api/assets/icons/flags/.
#
# Source: https://flagcdn.com — flag images derived from the public-domain
# `flag-icons` project (https://github.com/lipis/flag-icons). National flags are
# not copyrightable, so the bundled PNGs carry no licensing restriction.
#
# The files are keyed by ISO 3166-1 alpha-2 country code. The API maps a title's
# TMDB `original_language` (ISO 639-1) to a representative country (see
# `flag_country_for_lang` in api/src/image/icons.rs); languages without a mapped
# flag fall back to a text badge at render time.
#
# Re-run after changing the country list below to refresh the committed assets.

set -euo pipefail

# flagcdn width bucket. ~160px is crisp enough for the largest badge scale while
# keeping each PNG small (a few KB) for embedding via include_bytes!.
WIDTH="w160"

# ISO 3166-1 alpha-2 country codes to fetch (one flag per representative country).
COUNTRIES=(
    us jp kr cn fr de es it pt br ru in nl se dk no fi pl tr th id cz gr il hu
    ro ua vn ir my ph bd sa is ee lv lt sk si hr rs bg gb
)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$SCRIPT_DIR/../api/assets/icons/flags"
mkdir -p "$DEST"

echo ">> Downloading ${#COUNTRIES[@]} flags ($WIDTH) -> $DEST"
for cc in "${COUNTRIES[@]}"; do
    url="https://flagcdn.com/$WIDTH/$cc.png"
    out="$DEST/$cc.png"
    if curl -fsS --max-time 30 -o "$out" "$url"; then
        printf '   %s ' "$cc"
    else
        echo >&2 "!! failed to fetch $cc ($url)"
        exit 1
    fi
done
echo
echo ">> Done. $(ls "$DEST" | wc -l | tr -d ' ') flags in $DEST"
