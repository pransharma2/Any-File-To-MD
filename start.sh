#!/usr/bin/env sh
# Run me: ./start.sh   (first run installs everything automatically)
cd "$(dirname "$0")" || exit 1

if [ -x ".tools/uv" ]; then
  UV=".tools/uv"
elif command -v uv >/dev/null 2>&1; then
  UV="uv"
else
  echo "First-time setup: downloading a small helper (uv)..."
  curl -LsSf https://astral.sh/uv/install.sh | UV_INSTALL_DIR="$PWD/.tools" UV_NO_MODIFY_PATH=1 sh || exit 1
  UV=".tools/uv"
fi

exec "$UV" run --quiet --project . anytomd "$@"
