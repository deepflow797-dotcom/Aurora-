#!/usr/bin/env bash
# Fetch the published-crate mirror used by scripts/vendor/vendor.py.
set -euo pipefail
dest="${1:-/tmp/rust_vendor-main}"
if [ -d "$dest" ]; then
  echo "mirror already at $dest"
  exit 0
fi
tmp="$(mktemp -d)"
curl -sSL --fail https://codeload.github.com/ClickHouse/rust_vendor/tar.gz/refs/heads/main -o "$tmp/mirror.tgz"
tar xzf "$tmp/mirror.tgz" -C "$tmp"
mv "$tmp/rust_vendor-main" "$dest"
rm -rf "$tmp"
echo "mirror ready at $dest ($(ls "$dest" | wc -l) crates)"
