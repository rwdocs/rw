#!/usr/bin/env bash
set -euo pipefail

# 1. Fix ownership of named-volume mount points (root-owned by default)
sudo /usr/local/bin/devcontainer-prepare.sh

cd /workspace

# 2. rust-toolchain.toml triggers rustup to install 1.95.0 lazily; make it
#    explicit so install errors surface here rather than at first cargo build
rustup show

# 3. cargo-llvm-cov needs the llvm-tools-preview component
rustup component add llvm-tools-preview

# 4. Cargo dev tools (lands in rw-cargo-cache volume, persists across rebuilds)
cargo install --locked cargo-llvm-cov cargo-edit

# 5. Node deps (lands in rw-node-modules volume)
npm install

# 6. Playwright: install chromium plus its OS package deps in one shot.
#    Covers both `chromium` and `chromium-embedded` projects (same browser binary).
#    Run as `vscode` (NOT prefixed with sudo) so the browser binary lands in
#    /home/vscode/.cache/ms-playwright. Playwright self-elevates internally to
#    sudo for the apt portion; the base image grants `vscode` passwordless sudo,
#    so this works without any extra sudoers entries.
npx playwright install --with-deps chromium
