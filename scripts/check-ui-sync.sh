#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

diff -qr "$ROOT/packages/ui" "$ROOT/apps/mac/ui" >/dev/null || { echo "Mac UI is out of sync"; exit 1; }
diff -qr "$ROOT/packages/ui" "$ROOT/apps/windows/ui" >/dev/null || { echo "Windows UI is out of sync"; exit 1; }

echo "UI sync OK"
