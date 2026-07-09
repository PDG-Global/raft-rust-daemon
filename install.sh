#!/bin/bash
#
# Simple install script for raft-daemon.
#
# Usage:
#   curl -L https://github.com/PDG-Global/raft-rust-daemon/releases/latest/download/install.sh | bash
#   curl -L ... | bash -s -- --prefix ~/.local
#
# Detects the host OS/architecture, downloads the matching release binary,
# verifies its SHA-256 checksum, and installs it to /usr/local/bin by default
# (or the directory given by --prefix).
#

set -eo pipefail

REPO="PDG-Global/raft-rust-daemon"
DEFAULT_PREFIX="/usr/local"
PREFIX="$DEFAULT_PREFIX"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--prefix <dir>]"
            echo "  --prefix    Installation directory (default: $DEFAULT_PREFIX)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--prefix <dir>]"
            exit 1
            ;;
    esac
done

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
    Darwin)
        PLATFORM="macos"
        ;;
    Linux)
        PLATFORM="linux"
        ;;
    FreeBSD)
        PLATFORM="freebsd"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        case "$PLATFORM" in
            macos)   ASSET="raft-daemon-macos-x86_64" ;;
            linux)   ASSET="raft-daemon-x86_64-linux-static" ;;
            freebsd) ASSET="raft-daemon-x86_64-freebsd" ;;
        esac
        ;;
    arm64|aarch64)
        case "$PLATFORM" in
            macos)   ASSET="raft-daemon-macos-arm64" ;;
            linux)   ASSET="raft-daemon-aarch64-linux-static" ;;
            freebsd)
                echo "FreeBSD aarch64 binaries are only built on a FreeBSD host; use the x86_64 build or build from source."
                exit 1
                ;;
        esac
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
CHECKSUM_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}.sha256"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading ${ASSET}..."
HTTP_CODE=$(curl -fsSL -o "$TMP_DIR/raft-daemon" -w "%{http_code}" "$DOWNLOAD_URL" || true)
if [[ "$HTTP_CODE" != "200" ]]; then
    echo "Download failed (HTTP ${HTTP_CODE}). Asset may not exist: ${DOWNLOAD_URL}"
    exit 1
fi

echo "Verifying checksum..."
if command -v sha256sum &> /dev/null; then
    CHECKSUM_BIN="sha256sum"
else
    CHECKSUM_BIN="shasum -a 256"
fi

curl -fsSL "$CHECKSUM_URL" | awk '{print $1}' > "$TMP_DIR/expected.sha256"
$CHECKSUM_BIN "$TMP_DIR/raft-daemon" | awk '{print $1}' > "$TMP_DIR/actual.sha256"

if ! diff -q "$TMP_DIR/expected.sha256" "$TMP_DIR/actual.sha256" &> /dev/null; then
    echo "Checksum verification failed!"
    exit 1
fi

INSTALL_DIR="${PREFIX}/bin"
mkdir -p "$INSTALL_DIR"
chmod +x "$TMP_DIR/raft-daemon"

# Use sudo if installing to a system directory we don't own.
if [[ -w "$INSTALL_DIR" ]]; then
    mv "$TMP_DIR/raft-daemon" "$INSTALL_DIR/raft-daemon"
else
    echo "Installing to ${INSTALL_DIR} requires elevated privileges."
    sudo mv "$TMP_DIR/raft-daemon" "$INSTALL_DIR/raft-daemon"
fi

echo "raft-daemon installed to ${INSTALL_DIR}/raft-daemon"

if command -v raft-daemon &> /dev/null; then
    INSTALLED_VERSION=$(raft-daemon --version 2>/dev/null || echo "unknown")
    echo "Version: ${INSTALLED_VERSION}"
else
    echo "Add ${INSTALL_DIR} to your PATH to use raft-daemon."
fi
