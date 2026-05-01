#!/usr/bin/env bash
# Fix ownership of named-volume mount points. Volumes mount root-owned
# and empty by default; this lets the vscode user write to them.
set -euo pipefail
chown vscode:vscode /workspace/target /workspace/node_modules /usr/local/cargo /home/vscode/.claude
chmod 755 /workspace/target /workspace/node_modules /usr/local/cargo /home/vscode/.claude
