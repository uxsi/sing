#!/usr/bin/env bash
# Download pinned sing-box release binaries into Tauri externalBin layout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/third_party/sing-box/VERSION")"
OUT_DIR="$ROOT/apps/desktop/src-tauri/binaries"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$OUT_DIR"

# Map: release asset suffix -> Tauri target triple
declare -a PAIRS=(
  "darwin-arm64:aarch64-apple-darwin"
  "darwin-amd64:x86_64-apple-darwin"
)

HOST_ONLY=0
if [[ "${1:-}" == "--host" ]]; then
  HOST_ONLY=1
fi

host_triple() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os/$arch" in
    Darwin/arm64) echo "aarch64-apple-darwin" ;;
    Darwin/x86_64) echo "x86_64-apple-darwin" ;;
    Linux/x86_64) echo "x86_64-unknown-linux-gnu" ;;
    Linux/aarch64) echo "aarch64-unknown-linux-gnu" ;;
    *) echo "unsupported-host"; return 1 ;;
  esac
}

want_triple="$(host_triple || true)"

fetch_one() {
  local asset_suffix="$1"
  local triple="$2"
  local url="https://github.com/SagerNet/sing-box/releases/download/v${VERSION}/sing-box-${VERSION}-${asset_suffix}.tar.gz"
  local dest="$OUT_DIR/sing-box-${triple}"
  if [[ -x "$dest" ]]; then
    echo "skip (exists): $dest"
    return 0
  fi
  echo "fetch $url"
  curl -fsSL "$url" -o "$TMP/sb.tgz"
  mkdir -p "$TMP/extract"
  rm -rf "$TMP/extract"/*
  tar -xzf "$TMP/sb.tgz" -C "$TMP/extract"
  local bin
  bin="$(find "$TMP/extract" -type f -name sing-box | head -n1)"
  if [[ -z "$bin" ]]; then
    echo "sing-box binary not found in archive" >&2
    return 1
  fi
  cp "$bin" "$dest"
  chmod +x "$dest"
  echo "wrote $dest"
}

for pair in "${PAIRS[@]}"; do
  asset="${pair%%:*}"
  triple="${pair##*:}"
  if [[ "$HOST_ONLY" -eq 1 && "$triple" != "$want_triple" ]]; then
    continue
  fi
  fetch_one "$asset" "$triple"
done

echo "done. VERSION=$VERSION OUT=$OUT_DIR"
