#!/usr/bin/env bash
set -euo pipefail

if [[ "${RW_DEVCONTAINER_FIREWALL:-0}" != "1" ]]; then
  echo "[rw devcontainer] firewall: OFF (set RW_DEVCONTAINER_FIREWALL=1 on host before opening to enable)"
  exit 0
fi

echo "[rw devcontainer] firewall: ENABLING"
sudo /usr/local/bin/init-firewall.sh
