#!/bin/sh
set -eu

repo="Isllanrx/status_cli"
base="${STATUS_CLI_BASE_URL:-https://github.com/$repo/releases/latest/download}"
bin_dir="${STATUS_CLI_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
  echo "status_cli: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Linux)
    case "$(uname -m)" in
      x86_64 | amd64) asset="status_cli-x86_64-unknown-linux-musl" ;;
      aarch64 | arm64) asset="status_cli-aarch64-unknown-linux-musl" ;;
      *) fail "unsupported architecture: $(uname -m)" ;;
    esac
    ;;
  Darwin) asset="status_cli-universal-apple-darwin" ;;
  *) fail "unsupported system: $(uname -s); on Windows run install.ps1" ;;
esac

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q --tries=3 -O "$2" "$1"; }
else
  fail "curl or wget is required"
fi

if command -v sha256sum >/dev/null 2>&1; then
  digest() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  digest() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  fail "sha256sum or shasum is required"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "Downloading $asset"
fetch "$base/$asset" "$tmp/$asset" || fail "download failed: $base/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "download failed: $base/SHA256SUMS"

expected="$(awk -v name="$asset" '$2 == name { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || fail "no checksum published for $asset"
[ "$(digest "$tmp/$asset")" = "$expected" ] || fail "checksum mismatch for $asset"

mkdir -p "$bin_dir"
chmod +x "$tmp/$asset"
mv -f "$tmp/$asset" "$bin_dir/status_cli"
if [ "$(uname -s)" = "Darwin" ]; then
  xattr -d com.apple.quarantine "$bin_dir/status_cli" 2>/dev/null || true
fi
echo "Installed $("$bin_dir/status_cli" --version) to $bin_dir"

case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *)
    echo "Hosts call status_cli by its full path. To run it yourself, add $bin_dir to PATH:"
    echo "  echo 'export PATH=\"$bin_dir:\$PATH\"' >> ~/.profile"
    ;;
esac

install_font() {
  version="v3.5.1"
  archive="CascadiaCode.tar.xz"
  case "$(uname -s)" in
    Darwin) font_dir="$HOME/Library/Fonts" ;;
    *) font_dir="$HOME/.local/share/fonts" ;;
  esac
  [ -f "$font_dir/CaskaydiaCoveNerdFont-Regular.ttf" ] && return 0
  url="https://github.com/ryanoasis/nerd-fonts/releases/download/$version"
  fetch "$url/$archive" "$tmp/$archive" || return 1
  fetch "$url/SHA-256.txt" "$tmp/SHA-256.txt" || return 1
  expected="$(awk -v name="*$archive" '$2 == name || $2 == substr(name, 2) { print $1 }' "$tmp/SHA-256.txt")"
  [ -n "$expected" ] && [ "$(digest "$tmp/$archive")" = "$expected" ] || return 1
  mkdir -p "$tmp/font" "$font_dir"
  tar -xJf "$tmp/$archive" -C "$tmp/font" 2>/dev/null || return 1
  for style in Regular Bold Italic BoldItalic; do
    cp "$tmp/font/CaskaydiaCoveNerdFont-$style.ttf" "$font_dir/" || return 1
  done
  if command -v fc-cache >/dev/null 2>&1; then fc-cache -f "$font_dir" >/dev/null 2>&1 || true; fi
  echo "Installed CaskaydiaCove Nerd Font $version to $font_dir; select it in your terminal to see the icons"
}

if [ -z "${STATUS_CLI_NO_FONT:-}" ]; then
  install_font || echo "status_cli: Nerd Font not installed; the status line uses plain symbols instead of icons" >&2
fi

"$bin_dir/status_cli" setup
echo "Done. Open a new Claude Code or agy session to see the status line."
