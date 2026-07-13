#!/bin/sh
# yd installer and updater for Linux x86_64.
set -eu

REPOSITORY="9sx77ssl/yd"
INSTALL_DIR="${YD_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="yd"

say() { printf '%s\n' "$*"; }
fail() { say "yd installer: $*" >&2; exit 1; }
note() { say "yd installer: $*"; }

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

cleanup() {
  [ -n "${TEMP_DIR:-}" ] && [ -d "$TEMP_DIR" ] && rm -rf "$TEMP_DIR"
}

add_path_entry() {
  shell_file="$1"
  path_line='export PATH="$HOME/.local/bin:$PATH"'
  [ "$INSTALL_DIR" = "$HOME/.local/bin" ] || return 0
  [ -f "$shell_file" ] && grep -Fqx "$path_line" "$shell_file" && return 0
  printf '\n# yd\n%s\n' "$path_line" >> "$shell_file" || fail "could not update $shell_file"
  note "added ~/.local/bin to $(basename "$shell_file")"
}

installed_version() {
  [ -x "$INSTALL_DIR/$BINARY_NAME" ] || return 1
  "$INSTALL_DIR/$BINARY_NAME" --version 2>/dev/null | awk 'NR == 1 { print $2; exit }'
}

latest_field() {
  field="$1"
  sed -n "s/.*\"$field\": \"\([^\"]*\)\".*/\1/p" "$TEMP_DIR/release.json" | head -n 1
}

asset_url() {
  asset="$1"
  sed -n "s|.*\"browser_download_url\": \"\([^\"]*/$asset\)\".*|\1|p" "$TEMP_DIR/release.json" | head -n 1
}

verify_checksum() {
  expected="$(awk -v asset="$ARCHIVE" '$2 == asset { print $1; exit }' "$TEMP_DIR/SHA256SUMS")"
  [ -n "$expected" ] || fail "release checksum for $ARCHIVE is missing"
  actual="$(sha256sum "$TEMP_DIR/$ARCHIVE" | awk '{ print $1 }')"
  [ "$expected" = "$actual" ] || fail "checksum verification failed; installation aborted"
}

case "$(uname -s)" in
  Linux) ;;
  *) fail "yd currently provides Linux releases only" ;;
esac

case "$(uname -m)" in
  x86_64|amd64) TARGET="x86_64-unknown-linux-gnu" ;;
  *) fail "unsupported architecture: $(uname -m)" ;;
esac

require_command curl
require_command tar
require_command sha256sum
require_command awk
require_command sed
require_command install
require_command mktemp

TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/yd.XXXXXX")" || fail "could not create temporary directory"
trap cleanup EXIT HUP INT TERM

API_URL="https://api.github.com/repos/$REPOSITORY/releases/latest"
note "checking the latest yd release"
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 "$API_URL" -o "$TEMP_DIR/release.json" \
  || fail "could not fetch release metadata; check your internet connection"

LATEST_TAG="$(latest_field tag_name)"
[ -n "$LATEST_TAG" ] || fail "GitHub did not return a usable latest release"
LATEST_VERSION="${LATEST_TAG#v}"
CURRENT_VERSION="$(installed_version || true)"

add_path_entry "$HOME/.bashrc"
add_path_entry "$HOME/.zshrc"

if [ "$CURRENT_VERSION" = "$LATEST_VERSION" ]; then
  note "yd $CURRENT_VERSION is already up to date"
  exit 0
fi

ARCHIVE="yd-$TARGET.tar.gz"
ARCHIVE_URL="$(asset_url "$ARCHIVE")"
CHECKSUM_URL="$(asset_url SHA256SUMS)"
[ -n "$ARCHIVE_URL" ] || fail "release $LATEST_TAG has no $ARCHIVE asset"
[ -n "$CHECKSUM_URL" ] || fail "release $LATEST_TAG has no SHA256SUMS asset"

if [ -n "$CURRENT_VERSION" ]; then
  note "updating yd $CURRENT_VERSION to $LATEST_VERSION"
else
  note "installing yd $LATEST_VERSION"
fi

curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 "$ARCHIVE_URL" -o "$TEMP_DIR/$ARCHIVE" \
  || fail "could not download the yd archive"
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 "$CHECKSUM_URL" -o "$TEMP_DIR/SHA256SUMS" \
  || fail "could not download release checksums"
verify_checksum

tar -tzf "$TEMP_DIR/$ARCHIVE" | grep -Fxq "$BINARY_NAME" \
  || fail "release archive has an unexpected layout"
tar -xzf "$TEMP_DIR/$ARCHIVE" -C "$TEMP_DIR"
[ -f "$TEMP_DIR/$BINARY_NAME" ] || fail "release archive does not contain yd"

mkdir -p "$INSTALL_DIR" || fail "could not create $INSTALL_DIR"
install -m 0755 "$TEMP_DIR/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME" \
  || fail "could not install yd to $INSTALL_DIR"

note "yd $LATEST_VERSION installed at $INSTALL_DIR/$BINARY_NAME"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) say "Ready: yd -w" ;;
  *) say "Open a new terminal, then run: yd -w" ;;
esac
