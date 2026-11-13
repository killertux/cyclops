#!/bin/sh
# Installs Cyclops from its GitHub releases:
#
#   curl -fsSL https://raw.githubusercontent.com/killertux/cyclops/main/install.sh | sh
#
# It picks the build for this machine, checks its checksum, and puts `cyclops` in
# ~/.local/bin. These variables change what it does:
#
#   CYCLOPS_VERSION      a release tag, such as v0.1.0. The latest release by default.
#   CYCLOPS_INSTALL_DIR  where to put `cyclops`. ~/.local/bin by default.
#   CYCLOPS_RELEASES     where releases are downloaded from, for a mirror.
#
# Everything is inside functions that run on the last line, so a download cut short
# runs nothing.

set -eu

repository="killertux/cyclops"

say() {
    printf 'cyclops: %s\n' "$*"
}

fail() {
    printf 'cyclops: %s\n' "$*" >&2
    exit 1
}

# The build for this operating system and processor.
target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os/$arch" in
        Darwin/arm64 | Darwin/aarch64) echo "aarch64-apple-darwin" ;;
        Linux/x86_64 | Linux/amd64) echo "x86_64-unknown-linux-gnu" ;;
        Linux/aarch64 | Linux/arm64) echo "aarch64-unknown-linux-gnu" ;;
        *) fail "there is no build for $os on $arch. Build it from source: https://github.com/$repository#getting-started" ;;
    esac
}

# Downloads a URL into a file, with curl or wget.
download() {
    if command -v curl >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget --https-only -q "$1" -O "$2"
    else
        fail "curl or wget is needed to download Cyclops."
    fi
}

# The SHA-256 of a file, if this machine can work it out.
checksum() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

main() {
    target=$(target)
    version=${CYCLOPS_VERSION:-latest}
    releases=${CYCLOPS_RELEASES:-https://github.com/$repository/releases}
    install_dir=${CYCLOPS_INSTALL_DIR:-$HOME/.local/bin}
    archive="cyclops-$target.tar.gz"
    if [ "$version" = "latest" ]; then
        url="$releases/latest/download/$archive"
    else
        url="$releases/download/$version/$archive"
    fi

    temporary=$(mktemp -d)
    trap 'rm -rf "$temporary"' EXIT

    say "downloading $version for $target"
    download "$url" "$temporary/$archive" || fail "could not download $url"
    download "$url.sha256" "$temporary/$archive.sha256" || fail "could not download $url.sha256"

    expected=$(cut -d ' ' -f 1 "$temporary/$archive.sha256")
    actual=$(checksum "$temporary/$archive")
    if [ -z "$actual" ]; then
        say "sha256sum and shasum are missing, so the download is not checked"
    elif [ "$actual" != "$expected" ]; then
        fail "the download is damaged: its checksum is $actual, and $expected was expected"
    fi

    tar -xzf "$temporary/$archive" -C "$temporary"
    mkdir -p "$install_dir"
    mv "$temporary/cyclops-$target/cyclops" "$install_dir/cyclops"
    chmod 755 "$install_dir/cyclops"
    say "installed $("$install_dir/cyclops" --version) in $install_dir"

    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *) say "$install_dir is not on your PATH. Add it, for example in ~/.profile:
    export PATH=\"$install_dir:\$PATH\"" ;;
    esac
}

main "$@"
