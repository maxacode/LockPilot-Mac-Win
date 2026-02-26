#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SHARED="$ROOT/packages/ui"
MAC_UI="$ROOT/apps/mac/ui"
WIN_UI="$ROOT/apps/windows/ui"

sync_dir() {
  local src="$1"
  local dest="$2"

  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete "$src/" "$dest/"
    return
  fi

  rm -rf "$dest"
  mkdir -p "$dest"
  cp -a "$src/." "$dest/"
}

sync_dir "$SHARED" "$MAC_UI"
sync_dir "$SHARED" "$WIN_UI"

echo "Shared UI synced to mac + windows apps."
