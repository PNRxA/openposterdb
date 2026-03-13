#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

IMAGE_NAME="openposterdb-test"
CONTAINER_NAME="openposterdb-test"

if command -v podman &>/dev/null; then
    CTR=podman
elif command -v docker &>/dev/null; then
    CTR=docker
else
    echo "Error: neither podman nor docker found" >&2
    exit 1
fi

echo "=== Backend tests ==="
(cd api && cargo test)

echo ""
echo "=== Frontend unit tests ==="
(cd web && npx vitest run)

echo ""
echo "=== E2E tests ==="

cleanup() {
    echo "=== Tearing down ==="
    $CTR rm -f "$CONTAINER_NAME" 2>/dev/null || true
}
trap cleanup EXIT

echo "Building container image..."
$CTR build -t "$IMAGE_NAME" --build-arg CARGO_FEATURES=test-support -f Containerfile .

ENV_ARGS=()
if [ -f api/.env ]; then
    echo "Loading API keys from api/.env"
    while IFS='=' read -r key value; do
        [[ -z "$key" || "$key" == \#* ]] && continue
        ENV_ARGS+=(-e "$key=$value")
    done < api/.env
fi

echo "Starting container..."
$CTR rm -f "$CONTAINER_NAME" 2>/dev/null || true
$CTR run -d --name "$CONTAINER_NAME" \
    -p 3333:3000 \
    --tmpfs /tmp/openposterdb-e2e \
    "${ENV_ARGS[@]}" \
    -e JWT_SECRET=abababababababababababababababababababababababababababababababab \
    -e LISTEN_ADDR=0.0.0.0:3000 \
    -e COOKIE_SECURE=false \
    -e CACHE_DIR=/tmp/openposterdb-e2e \
    -e DB_DIR=/tmp/openposterdb-e2e \
    "$IMAGE_NAME"

echo "Waiting for backend..."
for i in $(seq 1 60); do
    if curl -sf http://127.0.0.1:3333/api/auth/status > /dev/null 2>&1; then
        echo "Backend ready"
        break
    fi
    if [ "$i" -eq 60 ]; then
        echo "Backend did not start within 60 seconds"
        $CTR logs "$CONTAINER_NAME"
        exit 1
    fi
    sleep 1
done

(cd web && npx playwright test --workers=1 --project=setup --project=settings --project=chromium --project=live)

echo ""
echo "All tests passed."
