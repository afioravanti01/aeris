#!/usr/bin/env bash
#
# deploy.sh — build aeris in release mode and install to $HOME/.local/bin

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

DEST="$HOME/.local/bin"

cargo build --release
mkdir -p "$DEST"
install -m 0755 target/release/aeris "$DEST/aeris"

echo "aeris installed at $DEST/aeris"
"$DEST/aeris" version

case ":$PATH:" in
    *":$DEST:"*) ;;
    *) echo "warn: $DEST is not in PATH; add: export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
esac
