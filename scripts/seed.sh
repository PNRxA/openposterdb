#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DATA_FILE="$SCRIPT_DIR/title.basics.tsv"

# ── defaults ──
LIMIT=100
LIMIT_PER_TYPE="" # if set, cap each type independently
TYPE=""          # "" = both, "movie", "tvSeries"
GENRES=""        # comma-separated, e.g. "Action,Horror"
YEAR_FROM=""
YEAR_TO="$(($(date +%Y) - 1))"
API_KEY="t0-free-rpdb"
CONCURRENCY=1
ASSETS="poster"  # poster, logo, backdrop, all
DRY_RUN=false

usage() {
  cat <<EOF
Usage: $(basename "$0") <BASE_URL> [OPTIONS]

Seed an OpenPosterDB instance by requesting posters to warm the cache.
Entries are processed newest-first (by year descending).
Sort year uses endYear for series (if available), startYear otherwise.

Optional:
  BASE_URL                 Server base URL (default: http://localhost:3000)

Options:
  -n, --limit NUM          Total entries to seed (default: $LIMIT, 0 = unlimited)
  -N, --limit-per-type NUM Cap each type independently (e.g. 100k movies + 100k series)
  -t, --type TYPE          Title type: movie, tv, or both (default: both)
  -g, --genres GENRES      Comma-separated genres to include (e.g. "Action,Horror")
  -f, --year-from YEAR     Minimum year (inclusive)
  -y, --year-to YEAR       Maximum year (inclusive, default: current year - 1)
  -k, --key KEY            API key to use (default: $API_KEY)
  -c, --concurrency NUM    Parallel requests (default: $CONCURRENCY)
                           WARNING: Each request may trigger upstream API calls
                           (TMDB, MDBList, OMDb). Higher concurrency risks hitting
                           their rate limits. Increase with caution.
  -a, --assets ASSETS      What to fetch: poster, logo, backdrop, all (default: $ASSETS)
  -d, --dry-run            Print what would be seeded without making requests
  -h, --help               Show this help

Examples:
  $(basename "$0") http://localhost:3000
  $(basename "$0") http://localhost:3000 -n 500 -t movie -f 2000
  $(basename "$0") http://localhost:3000 -n 0 -g "Horror,Thriller" -a all
  $(basename "$0") http://localhost:3000 -t tv -y 2020 -c 8
  $(basename "$0") http://localhost:3000 -N 100000
EOF
  exit 0
}

# ── parse args ──
if [[ "${1:-}" == "-h" ]] || [[ "${1:-}" == "--help" ]]; then
  usage
fi

# First arg is BASE_URL if it doesn't start with -
if [[ $# -gt 0 ]] && [[ "$1" != -* ]]; then
  BASE_URL="${1%/}"
  shift
else
  BASE_URL="http://localhost:3000"
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--limit)       LIMIT="$2";       shift 2 ;;
    -N|--limit-per-type) LIMIT_PER_TYPE="$2"; shift 2 ;;
    -t|--type)        TYPE="$2";        shift 2 ;;
    -g|--genres)      GENRES="$2";      shift 2 ;;
    -f|--year-from)   YEAR_FROM="$2";   shift 2 ;;
    -y|--year-to)     YEAR_TO="$2";     shift 2 ;;
    -k|--key)         API_KEY="$2";     shift 2 ;;
    -c|--concurrency) CONCURRENCY="$2"; shift 2 ;;
    -a|--assets)      ASSETS="$2";      shift 2 ;;
    -d|--dry-run)     DRY_RUN=true;     shift   ;;
    -h|--help)        usage ;;
    *) echo "Unknown option: $1"; usage ;;
  esac
done

if [[ ! -f "$DATA_FILE" ]]; then
  echo "Error: Data file not found at $DATA_FILE"
  echo "Download it from https://datasets.imdbws.com/title.basics.tsv.gz and extract it."
  exit 1
fi

# ── build awk filter & extract to temp file ──
# Columns: 1=tconst 2=titleType 3=primaryTitle 4=originalTitle
#          5=isAdult 6=startYear 7=endYear 8=runtimeMinutes 9=genres
#
# Output is tab-separated: sort_year \t tconst \t titleType \t primaryTitle

TMPFILE=$(mktemp)
trap 'rm -f "$TMPFILE"' EXIT

AWK_CONDS=()

case "$TYPE" in
  movie)  AWK_CONDS+=('$2 == "movie"') ;;
  tv)     AWK_CONDS+=('$2 == "tvSeries"') ;;
  both|"") ;;
  *)      echo "Error: --type must be movie, tv, or both"; exit 1 ;;
esac

if [[ -n "$YEAR_FROM" ]]; then
  AWK_CONDS+=("sort_year >= $YEAR_FROM")
fi

if [[ -n "$YEAR_TO" ]]; then
  AWK_CONDS+=("sort_year <= $YEAR_TO")
fi

if [[ -n "$GENRES" ]]; then
  IFS=',' read -ra GENRE_ARR <<< "$GENRES"
  GENRE_PARTS=()
  for g in "${GENRE_ARR[@]}"; do
    g=$(echo "$g" | xargs)
    GENRE_PARTS+=("index(\$9, \"$g\") > 0")
  done
  GENRE_COND=""
  for part in "${GENRE_PARTS[@]}"; do
    if [[ -n "$GENRE_COND" ]]; then
      GENRE_COND="$GENRE_COND || $part"
    else
      GENRE_COND="$part"
    fi
  done
  AWK_CONDS+=("($GENRE_COND)")
fi

# Join conditions with &&
AWK_FILTER="NR > 1"
for cond in "${AWK_CONDS[@]+"${AWK_CONDS[@]}"}"; do
  AWK_FILTER="$AWK_FILTER && $cond"
done

# Write awk script to a file to avoid shell escaping issues
AWK_SCRIPT=$(mktemp)
trap 'rm -f "$TMPFILE" "$AWK_SCRIPT"' EXIT

cat > "$AWK_SCRIPT" << 'AWKEOF'
BEGIN { FS="\t"; OFS="\t" }
NR > 1 {
  sort_year = ($7 != "\\N" && $7 + 0 > 0) ? $7 + 0 : ($6 != "\\N" ? $6 + 0 : 0)
AWKEOF

# Append the dynamic filter
echo "  if ($AWK_FILTER) print sort_year, \$1, \$2, \$3" >> "$AWK_SCRIPT"
echo "}" >> "$AWK_SCRIPT"

echo "Filtering titles..."
awk -f "$AWK_SCRIPT" "$DATA_FILE" | sort -t$'\t' -k1 -rn > "$TMPFILE"

TOTAL=$(wc -l < "$TMPFILE")

if [[ "$TOTAL" -eq 0 ]]; then
  echo "No entries match the given filters."
  exit 0
fi

# Apply limits: --limit-per-type takes precedence over --limit
if [[ -n "$LIMIT_PER_TYPE" ]]; then
  # Split by type, cap each independently, then merge back (still sorted by year desc)
  TMPFILE_LPT=$(mktemp)
  trap 'rm -f "$TMPFILE" "$AWK_SCRIPT" "$TMPFILE_LPT"' EXIT

  # Use awk to cap each type independently (avoids SIGPIPE from head)
  awk -F$'\t' -v cap="$LIMIT_PER_TYPE" '{
    count[$3]++
    if (count[$3] <= cap) print
  }' "$TMPFILE" > "$TMPFILE_LPT"

  # Re-sort merged results by year descending
  sort -t$'\t' -k1 -rn "$TMPFILE_LPT" > "$TMPFILE"
  rm -f "$TMPFILE_LPT"
  SELECTED=$(wc -l < "$TMPFILE")

  # Build per-type summary
  TYPE_SUMMARY=$(awk -F$'\t' '{count[$3]++} END {for (t in count) printf "%s: %d, ", t, count[t]}' "$TMPFILE")
  TYPE_SUMMARY="${TYPE_SUMMARY%, }"
  echo "Matched $TOTAL titles, seeding $SELECTED — $TYPE_SUMMARY (newest first, capped at $LIMIT_PER_TYPE per type)"
elif [[ "$LIMIT" -gt 0 ]] && [[ "$LIMIT" -lt "$TOTAL" ]]; then
  SELECTED="$LIMIT"
  echo "Matched $TOTAL titles, seeding $SELECTED (newest first)"
else
  SELECTED="$TOTAL"
  echo "Matched $TOTAL titles, seeding $SELECTED (newest first)"
fi
echo "Server:      $BASE_URL"
echo "API key:     $API_KEY"
echo "Assets:      $ASSETS"
echo "Concurrency: $CONCURRENCY"
echo ""

if [[ "$DRY_RUN" == true ]]; then
  echo "── Dry run ──"
  head -n "$SELECTED" "$TMPFILE" | while IFS=$'\t' read -r year id type title; do
    echo "  [$year] $type $id - $title"
  done
  exit 0
fi

# ── seeding ──
echo "── Seeding ──"

fetch_asset() {
  local base_url="$1" api_key="$2" id="$3" asset_type="$4" ext="$5"
  local url="${base_url}/${api_key}/imdb/${asset_type}-default/${id}.${ext}"
  curl -sf --max-time 60 -o /dev/null -w '%{time_total}' "$url" 2>/dev/null
}

COUNT=0
OK=0
FAIL=0

TOTAL_TIME=0

head -n "$SELECTED" "$TMPFILE" | while IFS=$'\t' read -r year id type title; do
  COUNT=$((COUNT + 1))
  result="OK"
  latency=""

  case "$ASSETS" in
    poster)
      latency=$(fetch_asset "$BASE_URL" "$API_KEY" "$id" "poster" "jpg") || result="FAIL"
      ;;
    logo)
      latency=$(fetch_asset "$BASE_URL" "$API_KEY" "$id" "logo" "png") || result="SKIP"
      ;;
    backdrop)
      latency=$(fetch_asset "$BASE_URL" "$API_KEY" "$id" "backdrop" "jpg") || result="SKIP"
      ;;
    all)
      latency=$(fetch_asset "$BASE_URL" "$API_KEY" "$id" "poster" "jpg") || result="FAIL"
      fetch_asset "$BASE_URL" "$API_KEY" "$id" "logo" "png" > /dev/null || true
      fetch_asset "$BASE_URL" "$API_KEY" "$id" "backdrop" "jpg" > /dev/null || true
      ;;
  esac

  if [[ "$result" == "OK" ]]; then
    OK=$((OK + 1))
  else
    FAIL=$((FAIL + 1))
  fi

  # Format latency as milliseconds
  if [[ -n "$latency" ]]; then
    ms=$(awk "BEGIN {printf \"%.0f\", $latency * 1000}")
    latency_str="${ms}ms"
  else
    latency_str="-"
  fi

  printf "[%d/%d] %-4s %6s  [%s] %-10s %s - %s\n" "$COUNT" "$SELECTED" "$result" "$latency_str" "$year" "$type" "$id" "$title"
done

echo ""
echo "Done."
