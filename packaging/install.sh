#!/bin/sh
# Offline installer, shipped beside the executable in each release archive.
set -eu

fail() { printf '%s\n' "$*" >&2; exit 1; }
prefix=${HOME:?HOME must be set}/.local
force=no
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix)
            [ "$#" -ge 2 ] || fail '--prefix requires an absolute directory'
            prefix=$2
            shift 2 ;;
        --force) force=yes; shift ;;
        --help)
            printf '%s\n' 'Usage: sh install.sh [--prefix /absolute/path] [--force]' \
                'Default prefix: ~/.local; installs bin/monkeytype-tui and share/doc/monkeytype-tui.' \
                'Does not download anything, use sudo, or edit shell configuration.'
            exit 0 ;;
        *) fail "Unknown option: $1" ;;
    esac
done
case "$prefix" in /*) ;; *) fail '--prefix must be an absolute directory' ;; esac
package_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
target=$(cat "$package_dir/release-target.txt")
case "$(uname -s):$(uname -m):$target" in
    Linux:x86_64:x86_64-unknown-linux-musl|Linux:x86_64:x86_64-unknown-linux-gnu|\
    Linux:aarch64:aarch64-unknown-linux-musl|Linux:aarch64:aarch64-unknown-linux-gnu|\
    Darwin:arm64:aarch64-apple-darwin|Darwin:x86_64:x86_64-apple-darwin) ;;
    *) fail 'This package does not match this operating system and CPU. Download the matching release archive.' ;;
esac
[ -f "$package_dir/monkeytype-tui" ] || fail 'Missing packaged executable'
[ -f "$package_dir/THIRD-PARTY-NOTICES.txt" ] || fail 'Missing dependency notices'
bindir=$prefix/bin
docdir=$prefix/share/doc/monkeytype-tui
destination=$bindir/monkeytype-tui
[ ! -L "$destination" ] || fail 'Refusing to replace a symbolic link'
if [ -e "$destination" ]; then
    [ -f "$destination" ] || fail 'Destination is not a regular file'
    [ "$force" = yes ] || fail 'Already installed; use --force to replace the executable'
fi
mkdir -p "$bindir" "$docdir"
staged=$(mktemp "$bindir/.monkeytype-tui.XXXXXX")
trap 'rm -f "$staged"' EXIT
trap 'exit 1' HUP INT TERM
cp "$package_dir/monkeytype-tui" "$staged"
chmod 755 "$staged"
cp "$package_dir/THIRD-PARTY-NOTICES.txt" "$docdir/THIRD-PARTY-NOTICES.txt"
cp "$package_dir/README.md" "$docdir/README.md"
if [ -f "$package_dir/LICENSE" ]; then
    cp "$package_dir/LICENSE" "$docdir/LICENSE"
fi
mv -f "$staged" "$destination"
printf 'Installed %s\n' "$destination"
case ":${PATH-}:" in
    *":$bindir:"*) printf '%s\n' 'Run: monkeytype-tui' ;;
    *) printf 'Add this directory to your shell PATH, then run monkeytype-tui: %s\n' "$bindir" ;;
esac
