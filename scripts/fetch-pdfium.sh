#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/src-tauri/resources/pdfium"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TARGET="${1:-current}"

mkdir -p "$OUT_DIR"

resolve_current_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os:$arch" in
    Darwin:arm64) echo "mac-arm64" ;;
    Darwin:x86_64) echo "mac-x64" ;;
    Linux:x86_64) echo "linux-x64" ;;
    MINGW*:x86_64|MSYS*:x86_64|CYGWIN*:x86_64) echo "windows-x64" ;;
    *)
      echo "Unsupported platform: $os $arch" >&2
      exit 1
      ;;
  esac
}

download_target() {
  local target="$1"
  local asset lib_name

  case "$target" in
    mac-arm64)
      asset="pdfium-mac-arm64.tgz"
      lib_name="libpdfium.dylib"
      ;;
    mac-x64)
      asset="pdfium-mac-x64.tgz"
      lib_name="libpdfium.dylib"
      ;;
    windows-x64)
      asset="pdfium-win-x64.tgz"
      lib_name="pdfium.dll"
      ;;
    linux-x64)
      asset="pdfium-linux-x64.tgz"
      lib_name="libpdfium.so"
      ;;
    *)
      echo "Usage: $0 [current|mac-arm64|mac-x64|windows-x64|linux-x64|all]" >&2
      exit 1
      ;;
  esac

  local work_dir="$TMP_DIR/$target"
  local url="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$asset"
  mkdir -p "$work_dir"

  echo "Downloading $url"
  curl -fL "$url" -o "$work_dir/pdfium.tgz"
  tar -xzf "$work_dir/pdfium.tgz" -C "$work_dir"

  local found
  found="$(find "$work_dir" -name "$lib_name" -type f | head -n 1)"
  if [[ -z "$found" ]]; then
    echo "Could not find $lib_name in $asset" >&2
    exit 1
  fi

  cp "$found" "$OUT_DIR/$lib_name"
  echo "Installed $OUT_DIR/$lib_name"
}

case "$TARGET" in
  current)
    download_target "$(resolve_current_target)"
    ;;
  all)
    download_target "mac-arm64"
    download_target "windows-x64"
    ;;
  *)
    download_target "$TARGET"
    ;;
esac
