#!/bin/sh
set -eu

OWNER_REPO="next-hat/nanocl"
API_URL="https://api.github.com/repos/$OWNER_REPO/releases/latest"
INSTALL_ROOT="$HOME/.nanocl"
ENV_FILE="$INSTALL_ROOT/env"
WRAPPER_MARK="# Added by Nanocl installer"

say() { printf '%s\n' "$*"; }
err() { printf 'Error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# http helpers
http_get() {
  if have curl; then
    curl -fsSL -H "Accept: application/vnd.github+json" "$1"
  elif have wget; then
    wget -qO- --header="Accept: application/vnd.github+json" "$1"
  else
    err "need curl or wget"
  fi
}

http_download() {
  url="$1"; out="$2"
  if have curl; then
    curl -fL "$url" -o "$out" >> /dev/null 2>&1
  elif have wget; then
    wget -qO "$out" "$url" >> /dev/null 2>&1
  else
    err "need curl or wget"
  fi
}

detect_platform() {
  OS="$(uname -s 2>/dev/null || echo unknown)"
  ARCH="$(uname -m 2>/dev/null || echo unknown)"

  case "$OS" in
    Linux)  os=linux ;;
    Darwin) os=mac ;;
    *) os=unknown ;;
  esac

  case "$ARCH" in
    x86_64|amd64) arch=amd64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) arch=unknown ;;
  esac

  printf '%s:%s\n' "$os" "$arch"
}

# match asset by pattern inside JSON's browser_download_url values
json_find_asset_url() {
  # Extract the first browser_download_url matching the provided ERE pattern (arg 1).
  # Uses only POSIX-ish grep/sed so it works on macOS (BSD) and Linux.
  # Reads JSON from stdin.
  pat="$1"
  # 1. Grep lines containing the key
  # 2. Sed to capture the URL
  # 3. Grep again to filter by pattern
  # 4. head -n1 to take first match
  # shellcheck disable=SC2002
  sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | \
    grep -E "$pat" | head -n 1
}

choose_asset_pattern() {
  plat="$1"
  case "$plat" in
    linux:amd64)   echo "nanocl_.*_linux_amd64.tar.gz" ;;
    mac:aarch64)   echo "nanocl_.*_mac_aarch64.tar.gz" ;;
    windows:amd64) echo "nanocl_.*_windows_amd64.tar.gz" ;; # kept for completeness
    *)             echo "" ;;
  esac
}

setup_env_file() {
  mkdir -p "$INSTALL_ROOT"

  cat > "$ENV_FILE" <<'EOF'
# Nanocl environment setup
export PATH="$HOME/.nanocl/bin:$PATH"
export MANPATH="$HOME/.nanocl/share/man:$MANPATH"
EOF

  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    bash) rcfile="$HOME/.bashrc" ;;
    zsh)  rcfile="$HOME/.zshrc" ;;
    *)    rcfile="$HOME/.profile" ;;
  esac

  # idempotent append of source line
  if [ ! -f "$rcfile" ] || ! grep -q '\. "$HOME/.nanocl/env"' "$rcfile" 2>/dev/null; then
    printf '\n%s\n[ -f "$HOME/.nanocl/env" ] && . "$HOME/.nanocl/env"\n' "$WRAPPER_MARK" >> "$rcfile"
    say "[*] Added Nanocl env source to $rcfile"
  else
    say "[*] Nanocl env already configured in $rcfile"
  fi
}

fallback_build_from_source() {
  say "[*] Falling back to building from source with cargo."
  if ! have cargo; then
    say "[*] Installing Rust via rustup…"
    curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs | sh -s -- -y
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  fi
  cargo install nanocl --root "$INSTALL_ROOT"
  setup_env_file
  say "[✓] Nanocl installed from source. Run: . \"$ENV_FILE\" or open a new shell"
  exit 0
}

ensure_macos_libpq() {
  # Ensure libpq is available on macOS since nanocl binary depends on PostgreSQL libs.
  if ! have sw_vers; then
    return 0
  fi
  if ! have brew; then
    say "[!] Homebrew not detected. To satisfy dependencies run:\n    brew install libpq\n    brew link --force libpq"
    return 0
  fi
  if ! brew ls --versions libpq >/dev/null 2>&1; then
    say "[*] Installing libpq via Homebrew"
    if ! brew install libpq; then
      say "[!] Failed to install libpq with Homebrew. You can try manually:\n    brew install libpq\n    brew link --force libpq"
      return 0
    fi
  else
    say "[*] libpq already installed"
  fi
  # Force link (Homebrew keeps libpq keg-only). Non-fatal if it fails.
  if brew link --force libpq >/dev/null 2>&1; then
    say "[*] Linked libpq"
  else
    say "[!] Could not force-link libpq (likely keg-only protections). Add its bin to PATH if needed: $(brew --prefix libpq)/bin"
  fi
}

main() {
  say "[*] Installing/updating nanocl in $INSTALL_ROOT"

  plat="$(detect_platform)"
  pattern="$(choose_asset_pattern "$plat")"
  if [ -z "$pattern" ]; then
    say "[!] No prebuilt asset pattern for platform: $plat"
    fallback_build_from_source
  fi

  say "[*] Detected platform: $plat"
  case "$plat" in
    mac:*) ensure_macos_libpq ;;
  esac
  json="$(http_get "$API_URL")" || err "failed to fetch release metadata"
  url="$(printf '%s' "$json" | json_find_asset_url "$pattern")"

  if [ -z "${url:-}" ]; then
    say "[!] Could not find matching asset in release JSON for pattern: $pattern"
    fallback_build_from_source
  fi

  say "[*] Downloading: $url"
  tmpdir="$(mktemp -d)" || err "failed to create tmpdir"
  trap 'rm -rf "$tmpdir"' EXIT INT HUP

  pkg="$tmpdir/nanocl.tar.gz"
  http_download "$url" "$pkg" || err "download failed"

  say "[*] Extracting archive to temporary directory..."
  # extract into tmpdir
  if have tar; then
    tar -xzf "$pkg" -C "$tmpdir" || err "failed to extract archive"
  else
    err "tar required to extract the release"
  fi

  # find the bin and share directories inside extracted tree
  bin_dir="$(find "$tmpdir" -type d -name bin -print -quit || true)"
  share_dir="$(find "$tmpdir" -type d -name share -print -quit || true)"

  if [ -z "$bin_dir" ] && [ -z "$share_dir" ]; then
    err "archive does not contain bin or share directories"
  fi

  # prepare install dir (do NOT remove whole INSTALL_ROOT; preserve other config)
  mkdir -p "$INSTALL_ROOT"

  # replace bin
  if [ -n "$bin_dir" ]; then
    say "[*] Updating $INSTALL_ROOT/bin"
    rm -rf "$INSTALL_ROOT/bin"
    # move the bin dir into place (rename)
    mv "$bin_dir" "$INSTALL_ROOT/bin" || {
      # fallback to copy if mv fails
      mkdir -p "$INSTALL_ROOT/bin"
      cp -r "$bin_dir"/* "$INSTALL_ROOT/bin/" || err "failed to copy bin files"
    }
    # ensure executables are executable (best-effort)
    if [ -f "$INSTALL_ROOT/bin/nanocl" ]; then
      chmod +x "$INSTALL_ROOT/bin/nanocl" || true
    fi
  else
    say "[!] No bin directory found in archive; skipping bin update"
  fi

  # replace share (manpages etc.)
  if [ -n "$share_dir" ]; then
    say "[*] Updating $INSTALL_ROOT/share"
    rm -rf "$INSTALL_ROOT/share"
    mv "$share_dir" "$INSTALL_ROOT/share" || {
      mkdir -p "$INSTALL_ROOT/share"
      cp -r "$share_dir"/* "$INSTALL_ROOT/share/" || err "failed to copy share files"
    }
  else
    say "[*] No share directory found in archive; skipping share update"
  fi

  # regenerate env and ensure shell rc sources it
  setup_env_file

  # cleanup happens via trap
  say "[✓] Installation/update complete."
  say "Run: . \"$ENV_FILE\" (or open a new shell) and try: nanocl --help"
}

main "$@"
