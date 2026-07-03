# Configuration

RW uses `rw.toml` for configuration. The file is automatically discovered in the current directory or any parent directory.

## Full config example

```toml
[server]
host = "127.0.0.1"      # Server host
port = 7979              # Server port (see "Port selection" below)

[docs]
source_dir = "docs"      # Markdown source directory
cache_enabled = true     # Enable/disable caching (default: true)

[diagrams]
kroki_url = "https://kroki.io"  # Optional; when absent, diagrams in markdown render as syntax-highlighted code (and `rw confluence render` emits a 'diagram skipped' warning).
include_dirs = ["."]            # PlantUML !include search paths
dpi = 192                       # DPI for diagrams (retina)

[live_reload]
enabled = true                  # Enable live reload (default: true)
watch_patterns = ["**/*.md"]    # Patterns to watch

[metadata]
name = "meta.yaml"              # Metadata file name (default: meta.yaml)
```

## Environment Variables

String configuration values support environment variable expansion:

```toml
[diagrams]
kroki_url = "${KROKI_URL:-https://kroki.io}"
```

Supported syntax:

- `${VAR}` -- expands to the value of `VAR`, errors if unset
- `${VAR:-default}` -- expands to `VAR` if set, otherwise uses `default`

Expandable fields: `server.host`, `diagrams.kroki_url`.

### `RW_DIAGRAMS_KROKI_URL` fallback

If no `rw.toml` (or no `[diagrams]` section) provides `diagrams.kroki_url`, RW reads `RW_DIAGRAMS_KROKI_URL` from the environment and uses it. This lets a project render diagrams without a config file:

```bash
export RW_DIAGRAMS_KROKI_URL="https://kroki.internal"
cd path/to/repo-without-rw-toml
rw serve
```

Precedence (highest to lowest):

1. `--kroki-url` CLI flag
2. `[diagrams] kroki_url` in `rw.toml` (with `${VAR}` expansion if used)
3. `RW_DIAGRAMS_KROKI_URL` environment variable
4. (none) -- diagram code blocks render as plain text

An empty value (`RW_DIAGRAMS_KROKI_URL=`) is treated as unset.

## CLI Overrides

CLI options override config file values:

```bash
# Use config file
rw serve

# Override port from config
rw serve --port 9000

# Use explicit config file
rw serve --config /path/to/rw.toml
```

## Port selection

`rw serve` listens on port `7979` by default. If that port is already in use
(for example, another `rw serve` is running), it automatically falls back to the
next free port — `7980`, `7981`, and so on — and prints the port it settled on:

```
Port 7979 is in use, using 7980 instead
Starting server on http://127.0.0.1:7980
```

Fallback applies **only to the default port**. When you set a port explicitly —
either `--port` on the command line or `[server].port` in `rw.toml` — it is
treated as a hard requirement: if that port is busy, `rw serve` fails with an
error instead of quietly using a different one.

## README.md as Homepage

If your `docs/` directory doesn't have an `index.md`, RW automatically uses `README.md` from the project root as the homepage. No configuration needed.

- `docs/index.md` exists: used as homepage (normal behavior)
- `docs/index.md` missing + `README.md` exists: README.md serves as homepage
- Live reload works for README.md changes too
