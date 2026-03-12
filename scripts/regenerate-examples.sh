#!/usr/bin/env bash
set -euo pipefail

BASE="${1:-http://localhost:3000}"
KEY="t0-free-rpdb"
OUT="$(dirname "$0")/../web/public/examples"

declare -A POSTERS=(
  [nosferatu]=tt0013442
  [metropolis]=tt0017136
  [caligari]=tt0010323
  [phantom]=tt0016220
  [trip-to-moon]=tt0000417
  [safety-last]=tt0014429
  [the-general]=tt0017925
)

echo "=== Posters ==="
for name in "${!POSTERS[@]}"; do
  id="${POSTERS[$name]}"
  echo -n "poster: $name ($id)... "
  curl -sf "$BASE/$KEY/imdb/poster-default/$id.jpg" -o "$OUT/$name.jpg"
  echo "OK"
done

echo "=== Logos ==="
for name in "${!POSTERS[@]}"; do
  id="${POSTERS[$name]}"
  echo -n "logo: $name ($id)... "
  if curl -sf "$BASE/$KEY/imdb/logo-default/$id.png" -o "$OUT/logo-$name.png"; then
    echo "OK"
  else
    rm -f "$OUT/logo-$name.png"
    echo "SKIP (not available)"
  fi
done

echo "=== Backdrops ==="
for name in "${!POSTERS[@]}"; do
  id="${POSTERS[$name]}"
  echo -n "backdrop: $name ($id)... "
  if curl -sf "$BASE/$KEY/imdb/backdrop-default/$id.jpg" -o "$OUT/backdrop-$name.jpg"; then
    echo "OK"
  else
    rm -f "$OUT/backdrop-$name.jpg"
    echo "SKIP (not available)"
  fi
done
