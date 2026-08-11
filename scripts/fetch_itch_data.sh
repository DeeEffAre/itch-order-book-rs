#!/usr/bin/env bash
set -euo pipefail

# Downloads a head subset of a real Nasdaq ITCH 5.0 daily file and writes the
# decompressed raw stream (length-prefixed messages) to disk for replay benches.
#
# Env overrides:
#   ITCH_DATA_DIR   output directory (default: $HOME/data/itch)
#   ITCH_SUBSET_BYTES  raw bytes to keep (default: 67108864 = 64 MiB)

URL="https://emi.nasdaq.com/itch/Nasdaq%20ITCH/01302020.NASDAQ_ITCH50.gz"
SIZE_BYTES="${ITCH_SUBSET_BYTES:-67108864}"
OUT_DIR="${ITCH_DATA_DIR:-$HOME/data/itch}"
OUT_FILE="$OUT_DIR/itch_subset.bin"
GZ_HEAD_BYTES=$((SIZE_BYTES * 3))

mkdir -p "$OUT_DIR"

echo "downloading first $GZ_HEAD_BYTES bytes of: $URL"
curl -sfL --range "0-$GZ_HEAD_BYTES" "$URL" -o "$OUT_DIR/.itch_head.gz"

echo "decompressing first $SIZE_BYTES bytes of the raw stream"
gzip -dc "$OUT_DIR/.itch_head.gz" | head -c "$SIZE_BYTES" > "$OUT_FILE"
rm -f "$OUT_DIR/.itch_head.gz"

echo "subset written to: $OUT_FILE"
ls -la "$OUT_FILE"
