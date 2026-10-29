#!/usr/bin/env bash
# Yusr v0.2.1 Resumable Phone Storage Downloader
# Downloads the official release package to /sdcard/Download/ with byte-level resume support.

set -e

DEST_DIR="/sdcard/Download"
VERSION="v0.2.1"
BASE_URL="https://github.com/zeiad6/yusr/releases/download/${VERSION}"

MSI_NAME="Yusr_0.2.1_x64_en-US.msi"
DEB_NAME="Yusr_0.2.1_amd64.deb"

mkdir -p "$DEST_DIR"

echo "=== Yusr ${VERSION} Resumable Phone Downloader ==="
echo "Target Folder: ${DEST_DIR}"

download_resumable() {
    local file="$1"
    local url="${BASE_URL}/${file}"
    local target="${DEST_DIR}/${file}"
    local sha_file="${file}.sha256"

    echo "--- Downloading ${file} (Resumable) ---"
    curl -L -C - --retry 15 --retry-delay 3 --progress-bar -o "$target" "$url"
    curl -sL -o "${DEST_DIR}/${sha_file}" "${BASE_URL}/${sha_file}"
    echo "Saved to: ${target}"
}

download_resumable "$MSI_NAME"
download_resumable "$DEB_NAME"

echo "=== Download Complete ==="
ls -lh "${DEST_DIR}/Yusr_0.2.1"*
