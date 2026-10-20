#!/usr/bin/env bash
set -euo pipefail

if [[ "${RUNNER_OS:-}" != Linux ]]; then
  echo "cargo-codspeed installation is only supported on Linux" >&2
  exit 1
fi

# Each architecture has its own version/digest pair so Renovate can update both.
case "${RUNNER_ARCH:-}" in
  X64)
    url="https://github.com/CodSpeedHQ/codspeed-rust/releases/download/v5.0.2/cargo-codspeed-x86_64-unknown-linux-musl"
    sha256="1ab86d023c6cb41a8b8356c4e60b145c6cb2ee23c8d6376be318149e813b32e0"
    ;;
  ARM64)
    url="https://github.com/CodSpeedHQ/codspeed-rust/releases/download/v5.0.2/cargo-codspeed-aarch64-unknown-linux-musl"
    sha256="98304c75a488a6e9bfe17e45d4ec51363a837455cb57babf0e0af2bfe88689e8"
    ;;
  *)
    echo "Unsupported runner architecture: ${RUNNER_ARCH:-unset}" >&2
    exit 1
    ;;
esac

: "${RUNNER_TEMP:?}" "${GITHUB_PATH:?}"
download_dir=$(mktemp -d "${RUNNER_TEMP}/cargo-codspeed.XXXXXX")
trap 'rm -rf "$download_dir"' EXIT
curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
  "$url" --output "$download_dir/cargo-codspeed"
printf '%s  %s\n' "$sha256" "$download_dir/cargo-codspeed" | sha256sum --check --strict

# Replace any cached binary in Cargo's own search path, not just a PATH shadow.
bin_dir="${CARGO_HOME:-$HOME/.cargo}/bin"
mkdir -p "$bin_dir"
install -m 755 "$download_dir/cargo-codspeed" "$bin_dir/cargo-codspeed"
printf '%s\n' "$bin_dir" >> "$GITHUB_PATH"
