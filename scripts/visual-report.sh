#!/usr/bin/env bash
# Generate a visual HTML report of all image permutations.
# Expects the test container to already be running on port 3333.
set -euo pipefail

cd "$(dirname "$0")/.."

BASE_URL="${BASE_URL:-http://127.0.0.1:3333}"
API_KEY="${API_KEY:-t0-free-rpdb}"
REPORT_DIR="test-report"
IMG_DIR="$REPORT_DIR/images"

rm -rf "$REPORT_DIR"
mkdir -p "$IMG_DIR"

# Test IDs
IMDB_ID="tt0111161"
EPISODE_ID="tt0529483"

PASS=0
FAIL=0

# Download an image and return the relative path, or empty string on failure
fetch_image() {
    local label="$1"
    local endpoint="$2"
    local query="$3"
    local ext="$4"
    # Sanitise label for filename
    local filename
    filename="$(echo "$label" | tr ' /=' '_' | tr -cd 'a-zA-Z0-9_-').${ext}"
    local id_type="${5:-imdb}"
    local id_value="${6:-$IMDB_ID}"
    local url="${BASE_URL}/${API_KEY}/${id_type}/${endpoint}/${id_value}.${ext}"
    if [ -n "$query" ]; then
        url="${url}?${query}"
    fi
    if curl -sf -o "${IMG_DIR}/${filename}" "$url"; then
        PASS=$((PASS + 1))
        echo "images/${filename}"
    else
        FAIL=$((FAIL + 1))
        echo ""
    fi
}

# Start building the HTML
REPORT_FILE="$REPORT_DIR/index.html"

# We'll collect all sections in a variable
SECTIONS=""

add_section() {
    local title="$1"
    SECTIONS="${SECTIONS}<h2>${title}</h2><div class=\"grid\">"
}

end_section() {
    SECTIONS="${SECTIONS}</div>"
}

add_image() {
    local label="$1"
    local path="$2"
    if [ -n "$path" ]; then
        SECTIONS="${SECTIONS}<figure><img src=\"${path}\" loading=\"lazy\"><figcaption>${label}</figcaption></figure>"
    else
        SECTIONS="${SECTIONS}<figure><div class=\"error\">Failed</div><figcaption>${label}</figcaption></figure>"
    fi
}

echo "Fetching poster permutations..."

# --- Posters ---

add_section "Poster - Default"
path=$(fetch_image "default" "poster-default" "" "jpg")
add_image "default" "$path"
end_section

add_section "Poster - badge_style"
for val in h v d; do
    path=$(fetch_image "badge_style=$val" "poster-default" "badge_style=$val" "jpg")
    add_image "badge_style=$val" "$path"
done
end_section

add_section "Poster - label_style"
for val in t i o; do
    path=$(fetch_image "label_style=$val" "poster-default" "label_style=$val" "jpg")
    add_image "label_style=$val" "$path"
done
end_section

add_section "Poster - badge_size"
for val in xs s m l xl; do
    path=$(fetch_image "badge_size=$val" "poster-default" "badge_size=$val" "jpg")
    add_image "badge_size=$val" "$path"
done
end_section

add_section "Poster - font_size"
for val in xs s m l xl; do
    path=$(fetch_image "font_size=$val" "poster-default" "font_size=$val" "jpg")
    add_image "font_size=$val" "$path"
done
end_section

add_section "Poster - badge_direction"
for val in h v d; do
    path=$(fetch_image "badge_direction=$val" "poster-default" "badge_direction=$val" "jpg")
    add_image "badge_direction=$val" "$path"
done
end_section

add_section "Poster - position"
for val in bc tc l r tl tr bl br; do
    path=$(fetch_image "position=$val" "poster-default" "position=$val" "jpg")
    add_image "position=$val" "$path"
done
end_section

add_section "Poster - textless"
for val in true false; do
    path=$(fetch_image "textless=$val" "poster-default" "textless=$val" "jpg")
    add_image "textless=$val" "$path"
done
end_section

add_section "Poster - image_source"
for val in t f; do
    path=$(fetch_image "image_source=$val" "poster-default" "image_source=$val" "jpg")
    add_image "image_source=$val" "$path"
done
end_section

add_section "Poster - ratings_limit"
for val in 0 1 3 5 8; do
    path=$(fetch_image "ratings_limit=$val" "poster-default" "ratings_limit=$val" "jpg")
    add_image "ratings_limit=$val" "$path"
done
end_section

add_section "Poster - imageSize"
for val in medium large; do
    path=$(fetch_image "imageSize=$val" "poster-default" "imageSize=$val" "jpg")
    add_image "imageSize=$val" "$path"
done
end_section

# Combined poster permutations: position × badge_direction
add_section "Poster - position × badge_direction"
for pos in bc tc l r tl tr bl br; do
    for dir in h v; do
        path=$(fetch_image "pos=${pos}_dir=${dir}" "poster-default" "position=${pos}&badge_direction=${dir}" "jpg")
        add_image "position=$pos badge_direction=$dir" "$path"
    done
done
end_section

# Combined poster permutations: badge_style × label_style
add_section "Poster - badge_style × label_style"
for bs in h v; do
    for ls in t i o; do
        path=$(fetch_image "bs=${bs}_ls=${ls}" "poster-default" "badge_style=${bs}&label_style=${ls}" "jpg")
        add_image "badge_style=$bs label_style=$ls" "$path"
    done
done
end_section

# Combined: position × badge_size
add_section "Poster - position × badge_size"
for pos in bc tc l r; do
    for sz in xs s m l xl; do
        path=$(fetch_image "pos=${pos}_sz=${sz}" "poster-default" "position=${pos}&badge_size=${sz}" "jpg")
        add_image "position=$pos badge_size=$sz" "$path"
    done
done
end_section

echo "Fetching logo permutations..."

# --- Logos ---

add_section "Logo - Default"
path=$(fetch_image "logo_default" "logo-default" "" "png")
add_image "default" "$path"
end_section

add_section "Logo - badge_style"
for val in h v d; do
    path=$(fetch_image "logo_badge_style=$val" "logo-default" "badge_style=$val" "png")
    add_image "badge_style=$val" "$path"
done
end_section

add_section "Logo - label_style"
for val in t i o; do
    path=$(fetch_image "logo_label_style=$val" "logo-default" "label_style=$val" "png")
    add_image "label_style=$val" "$path"
done
end_section

add_section "Logo - badge_size"
for val in xs s m l xl; do
    path=$(fetch_image "logo_badge_size=$val" "logo-default" "badge_size=$val" "png")
    add_image "badge_size=$val" "$path"
done
end_section

add_section "Logo - font_size"
for val in xs s m l xl; do
    path=$(fetch_image "logo_font_size=$val" "logo-default" "font_size=$val" "png")
    add_image "font_size=$val" "$path"
done
end_section

add_section "Logo - image_source"
for val in t f; do
    path=$(fetch_image "logo_image_source=$val" "logo-default" "image_source=$val" "png")
    add_image "image_source=$val" "$path"
done
end_section

add_section "Logo - ratings_limit"
for val in 0 1 3 5 8; do
    path=$(fetch_image "logo_ratings_limit=$val" "logo-default" "ratings_limit=$val" "png")
    add_image "ratings_limit=$val" "$path"
done
end_section

add_section "Logo - imageSize"
for val in medium large; do
    path=$(fetch_image "logo_imageSize=$val" "logo-default" "imageSize=$val" "png")
    add_image "imageSize=$val" "$path"
done
end_section

# Combined logo: badge_style × label_style
add_section "Logo - badge_style × label_style"
for bs in h v; do
    for ls in t i o; do
        path=$(fetch_image "logo_bs=${bs}_ls=${ls}" "logo-default" "badge_style=${bs}&label_style=${ls}" "png")
        add_image "badge_style=$bs label_style=$ls" "$path"
    done
done
end_section

echo "Fetching backdrop permutations..."

# --- Backdrops ---

add_section "Backdrop - Default"
path=$(fetch_image "backdrop_default" "backdrop-default" "" "jpg")
add_image "default" "$path"
end_section

add_section "Backdrop - badge_style"
for val in h v d; do
    path=$(fetch_image "backdrop_badge_style=$val" "backdrop-default" "badge_style=$val" "jpg")
    add_image "badge_style=$val" "$path"
done
end_section

add_section "Backdrop - label_style"
for val in t i o; do
    path=$(fetch_image "backdrop_label_style=$val" "backdrop-default" "label_style=$val" "jpg")
    add_image "label_style=$val" "$path"
done
end_section

add_section "Backdrop - badge_size"
for val in xs s m l xl; do
    path=$(fetch_image "backdrop_badge_size=$val" "backdrop-default" "badge_size=$val" "jpg")
    add_image "badge_size=$val" "$path"
done
end_section

add_section "Backdrop - font_size"
for val in xs s m l xl; do
    path=$(fetch_image "backdrop_font_size=$val" "backdrop-default" "font_size=$val" "jpg")
    add_image "font_size=$val" "$path"
done
end_section

add_section "Backdrop - badge_direction"
for val in h v d; do
    path=$(fetch_image "backdrop_badge_direction=$val" "backdrop-default" "badge_direction=$val" "jpg")
    add_image "badge_direction=$val" "$path"
done
end_section

add_section "Backdrop - position"
for val in bc tc l r tl tr bl br; do
    path=$(fetch_image "backdrop_position=$val" "backdrop-default" "position=$val" "jpg")
    add_image "position=$val" "$path"
done
end_section

add_section "Backdrop - image_source"
for val in t f; do
    path=$(fetch_image "backdrop_image_source=$val" "backdrop-default" "image_source=$val" "jpg")
    add_image "image_source=$val" "$path"
done
end_section

add_section "Backdrop - ratings_limit"
for val in 0 1 3 5 8; do
    path=$(fetch_image "backdrop_ratings_limit=$val" "backdrop-default" "ratings_limit=$val" "jpg")
    add_image "ratings_limit=$val" "$path"
done
end_section

add_section "Backdrop - imageSize"
for val in small medium large; do
    path=$(fetch_image "backdrop_imageSize=$val" "backdrop-default" "imageSize=$val" "jpg")
    add_image "imageSize=$val" "$path"
done
end_section

# Combined backdrop: position × badge_direction (needs multiple ratings to see direction)
add_section "Backdrop - position × badge_direction"
for pos in bc tc l r tl tr bl br; do
    for dir in h v; do
        path=$(fetch_image "backdrop_pos=${pos}_dir=${dir}" "backdrop-default" "position=${pos}&badge_direction=${dir}" "jpg")
        add_image "position=$pos badge_direction=$dir" "$path"
    done
done
end_section

# Combined backdrop: badge_style × label_style
add_section "Backdrop - badge_style × label_style"
for bs in h v; do
    for ls in t i o; do
        path=$(fetch_image "backdrop_bs=${bs}_ls=${ls}" "backdrop-default" "badge_style=${bs}&label_style=${ls}" "jpg")
        add_image "badge_style=$bs label_style=$ls" "$path"
    done
done
end_section

echo "Fetching episode permutations..."

# --- Episodes ---

add_section "Episode - Default"
path=$(fetch_image "episode_default" "episode-default" "ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
add_image "default" "$path"
end_section

add_section "Episode - badge_style"
for val in h v d; do
    path=$(fetch_image "episode_badge_style=$val" "episode-default" "badge_style=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "badge_style=$val" "$path"
done
end_section

add_section "Episode - label_style"
for val in t i o; do
    path=$(fetch_image "episode_label_style=$val" "episode-default" "label_style=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "label_style=$val" "$path"
done
end_section

add_section "Episode - badge_size"
for val in xs s m l xl; do
    path=$(fetch_image "episode_badge_size=$val" "episode-default" "badge_size=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "badge_size=$val" "$path"
done
end_section

add_section "Episode - font_size"
for val in xs s m l xl; do
    path=$(fetch_image "episode_font_size=$val" "episode-default" "font_size=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "font_size=$val" "$path"
done
end_section

add_section "Episode - badge_direction"
for val in h v d; do
    path=$(fetch_image "episode_badge_direction=$val" "episode-default" "badge_direction=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "badge_direction=$val" "$path"
done
end_section

add_section "Episode - position"
for val in bc tc l r tl tr bl br; do
    path=$(fetch_image "episode_position=$val" "episode-default" "position=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "position=$val" "$path"
done
end_section

add_section "Episode - blur"
for val in true false; do
    path=$(fetch_image "episode_blur=$val" "episode-default" "blur=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "blur=$val" "$path"
done
end_section

add_section "Episode - ratings_limit"
for val in 0 1 3 5 8; do
    path=$(fetch_image "episode_ratings_limit=$val" "episode-default" "ratings_limit=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "ratings_limit=$val" "$path"
done
end_section

add_section "Episode - imageSize"
for val in medium large; do
    path=$(fetch_image "episode_imageSize=$val" "episode-default" "imageSize=$val&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
    add_image "imageSize=$val" "$path"
done
end_section

# Combined episode: position × badge_direction (needs 2 ratings to see direction)
add_section "Episode - position × badge_direction"
for pos in bc tc l r tl tr bl br; do
    for dir in h v; do
        path=$(fetch_image "ep_pos=${pos}_dir=${dir}" "episode-default" "position=${pos}&badge_direction=${dir}&ratings_limit=2&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
        add_image "position=$pos badge_direction=$dir" "$path"
    done
done
end_section

# Combined episode: badge_style × label_style
add_section "Episode - badge_style × label_style"
for bs in h v; do
    for ls in t i o; do
        path=$(fetch_image "ep_bs=${bs}_ls=${ls}" "episode-default" "badge_style=${bs}&label_style=${ls}&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
        add_image "badge_style=$bs label_style=$ls" "$path"
    done
done
end_section

# Combined episode: blur × badge_style
add_section "Episode - blur × badge_style"
for blur in true false; do
    for bs in h v d; do
        path=$(fetch_image "ep_blur=${blur}_bs=${bs}" "episode-default" "blur=${blur}&badge_style=${bs}&ratings_order=imdb,tmdb" "jpg" "imdb" "$EPISODE_ID")
        add_image "blur=$blur badge_style=$bs" "$path"
    done
done
end_section

echo "Generating HTML report..."

cat > "$REPORT_FILE" <<'HTMLHEAD'
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>OpenPosterDB Visual Test Report</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: system-ui, sans-serif; background: #111; color: #eee; padding: 2rem; }
  h1 { margin-bottom: 1rem; }
  h2 { margin: 2rem 0 1rem; border-bottom: 1px solid #333; padding-bottom: 0.5rem; }
  .grid { display: flex; flex-wrap: wrap; gap: 1rem; }
  figure { background: #222; border-radius: 8px; padding: 0.5rem; max-width: 320px; }
  figure img { max-width: 100%; height: auto; border-radius: 4px; display: block; }
  figcaption { font-size: 0.85rem; color: #aaa; margin-top: 0.4rem; text-align: center; font-family: monospace; }
  .error { background: #400; color: #f88; padding: 2rem; text-align: center; border-radius: 4px; }
  .meta { color: #888; margin-bottom: 1rem; font-size: 0.9rem; }
  .summary { margin: 1rem 0; padding: 1rem; background: #222; border-radius: 8px; font-family: monospace; }
  .summary .pass { color: #4c4; }
  .summary .fail { color: #f44; }
</style>
</head>
<body>
<h1>OpenPosterDB Visual Test Report</h1>
<p class="meta">Generated at TIMESTAMP_PLACEHOLDER using free API key (t0-free-rpdb) against tt0111161 (The Shawshank Redemption) and tt0529483 (Bonanza S1E1)</p>
<div class="summary">PASS_PLACEHOLDER passed, FAIL_PLACEHOLDER failed out of TOTAL_PLACEHOLDER images</div>
HTMLHEAD

# Replace placeholders
TIMESTAMP=$(date -Iseconds)
TOTAL=$((PASS + FAIL))
sed -i "s/TIMESTAMP_PLACEHOLDER/$TIMESTAMP/" "$REPORT_FILE"
sed -i "s/PASS_PLACEHOLDER/<span class=\"pass\">$PASS<\/span>/" "$REPORT_FILE"
sed -i "s/FAIL_PLACEHOLDER/<span class=\"fail\">$FAIL<\/span>/" "$REPORT_FILE"
sed -i "s/TOTAL_PLACEHOLDER/$TOTAL/" "$REPORT_FILE"

# Append sections
echo "$SECTIONS" >> "$REPORT_FILE"

cat >> "$REPORT_FILE" <<'HTMLFOOT'
</body>
</html>
HTMLFOOT

echo ""
echo "Visual report: $REPORT_DIR/index.html ($PASS passed, $FAIL failed out of $TOTAL)"
