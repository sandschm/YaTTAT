#!/usr/bin/env bash
# Regenerates docs/screenshots/*.png from the browser mock (macOS only, needs Xcode CLT).
# Start the mock first:  npm run dev   (http://localhost:1421)
set -euo pipefail
cd "$(dirname "$0")/.."
BASE="${1:-http://localhost:1421}"
OUT=docs/screenshots
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

swiftc -O -o "$TMP/snap" scripts/snap.swift
mkdir -p "$OUT"

# shoot <name> <query> [cssWidth] [cssHeight] → $TMP/<name>.png at 2x (CSS px × 2)
shoot() {
  local name=$1 query=$2 w=${3:-1240} h=${4:-800}
  "$TMP/snap" "$BASE/$query" "$w" "$h" "$TMP/raw-$name.png" 2.5 >/dev/null
  sips --resampleWidth $((w * 2)) "$TMP/raw-$name.png" --out "$TMP/$name.png" >/dev/null
}
# crop <src> <dest> <x> <y> <w> <h>   (CSS px, applied to the 2x image)
crop() {
  local src=$1 dest=$2 x=$3 y=$4 w=$5 h=$6
  sips --cropToHeightWidth $((h * 2)) $((w * 2)) --cropOffset $((y * 2)) $((x * 2)) "$src" --out "$dest" >/dev/null
  echo "$dest"
}

shoot team "?view=team"
shoot member "?view=member"
shoot tickets "?view=tickets"
shoot connection "?view=settings&tab=connection"
shoot teamsettings "?view=settings&tab=team"

# Full windows (1240×800 CSS px) for the header images.
cp "$TMP/team.png" "$OUT/team.png"
cp "$TMP/member.png" "$OUT/member.png"
cp "$TMP/tickets.png" "$OUT/tickets.png"
echo "$OUT/team.png $OUT/member.png $OUT/tickets.png"
# Content-area crops (x ≥ 200 skips the sidebar) for the quick guide.
crop "$TMP/connection.png" "$OUT/step-1-verbinden.png" 200 0 1040 560
crop "$TMP/teamsettings.png" "$OUT/step-2-team.png" 200 0 1040 540
crop "$TMP/team.png" "$OUT/step-3-zeitraum.png" 200 0 1040 120
crop "$TMP/team.png" "$OUT/step-4-team.png" 200 120 1040 600
crop "$TMP/member.png" "$OUT/step-5-mitglied.png" 200 120 1040 560
crop "$TMP/tickets.png" "$OUT/step-6-tickets.png" 200 120 1040 560
