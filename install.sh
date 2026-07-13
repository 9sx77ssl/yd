#!/bin/sh
# yd installer/updater. It downloads release binaries rather than source code.
set -eu
repo="9sx77ssl/yd"
bin_dir="${YD_INSTALL_DIR:-$HOME/.local/bin}"
say() { printf '%s\n' "$*"; }
die() { say "yd installer: $*" >&2; exit 1; }
add_to_path() {
  file="$1"
  line='export PATH="$HOME/.local/bin:$PATH"'
  [ "$bin_dir" = "$HOME/.local/bin" ] || return 0
  [ -f "$file" ] && grep -Fqx "$line" "$file" && return 0
  printf '\n# yd\n%s\n' "$line" >> "$file" || die "could not update $file"
  say "Added yd to PATH in $file"
}
command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar >/dev/null 2>&1 || die "tar is required"
case "$(uname -s)" in Linux) ;; *) die "yd currently provides Linux releases only" ;; esac
case "$(uname -m)" in x86_64|amd64) target="x86_64-unknown-linux-gnu" ;; *) die "unsupported architecture: $(uname -m)" ;; esac
asset="yd-$target.tar.gz"
url="$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n "s|.*\"browser_download_url\": \"\([^\"]*$asset\)\".*|\1|p" | head -n 1)"
[ -n "$url" ] || die "no compatible release is available"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
say "Installing yd from the latest release..."
curl -fsSL "$url" -o "$tmp/yd.tar.gz"
tar -xzf "$tmp/yd.tar.gz" -C "$tmp"
[ -x "$tmp/yd" ] || die "release archive does not contain an executable yd"
mkdir -p "$bin_dir"
install -m 0755 "$tmp/yd" "$bin_dir/yd"
add_to_path "$HOME/.bashrc"
add_to_path "$HOME/.zshrc"
say "yd installed to $bin_dir/yd"
case ":$PATH:" in
  *":$bin_dir:"*) say "Ready: yd --wallet" ;;
  *) say "Open a new terminal, then run: yd --wallet" ;;
esac
