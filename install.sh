#!/bin/sh
# Bloom installer — https://github.com/hindol/Bloom
# Usage: curl -fsSL https://raw.githubusercontent.com/hindol/Bloom/main/install.sh | sh
set -e

REPO="hindol/Bloom"
INSTALL_DIR="${BLOOM_INSTALL_DIR:-$HOME/.local/bin}"

get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=1" \
        | grep '"tag_name"' | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//'
}

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Darwin)
            case "$arch" in
                arm64)   echo "aarch64-apple-darwin" ;;
                *)       echo "Error: only Apple Silicon (ARM64) macOS is supported" >&2; exit 1 ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64)  echo "x86_64-unknown-linux-gnu" ;;
                *)       echo "Error: unsupported architecture $arch on Linux" >&2; exit 1 ;;
            esac
            ;;
        *)
            echo "Error: unsupported OS $os (use install.ps1 for Windows)" >&2
            exit 1
            ;;
    esac
}

main() {
    version="${1:-$(get_latest_version)}"
    if [ -z "$version" ]; then
        echo "Error: could not determine latest version" >&2
        exit 1
    fi

    target=$(detect_target)
    url="https://github.com/${REPO}/releases/download/${version}/bloom-${version}-${target}.tar.gz"

    echo "Installing Bloom ${version} (${target})..."
    echo "  from: ${url}"
    echo "  to:   ${INSTALL_DIR}/bloom"

    mkdir -p "$INSTALL_DIR"

    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fsSL "$url" -o "$tmpdir/bloom.tar.gz"
    tar xzf "$tmpdir/bloom.tar.gz" -C "$tmpdir"
    install -m 755 "$tmpdir/bloom-gui" "$INSTALL_DIR/bloom"

    echo ""
    echo "✓ Bloom installed to ${INSTALL_DIR}/bloom"

    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            echo "⚠ ${INSTALL_DIR} is not in your PATH. Add it:"
            echo ""
            echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
            echo ""
            ;;
    esac
}

main "$@"
