# Configuration

RW uses `rw.toml` for configuration. The file is automatically discovered in the current directory or any parent directory.

## Full config example

```toml
[server]
host = "127.0.0.1"      # Server host
port = 7979              # Server port

[docs]
source_dir = "docs"      # Markdown source directory
cache_enabled = true     # Enable/disable caching (default: true)

[diagrams]
kroki_url = "https://kroki.io"  # Required when [diagrams] section is present
include_dirs = ["."]            # PlantUML !include search paths
dpi = 192                       # DPI for diagrams (retina)

[live_reload]
enabled = true                  # Enable live reload (default: true)
watch_patterns = ["**/*.md"]    # Patterns to watch

[metadata]
name = "meta.yaml"              # Metadata file name (default: meta.yaml)

[confluence]
base_url = "https://confluence.example.com"
access_token = "your-token"
access_secret = "your-secret"
consumer_key = "rw"
```

## Environment Variables

String configuration values support environment variable expansion:

```toml
[confluence]
base_url = "${CONFLUENCE_URL}"
access_token = "${CONFLUENCE_TOKEN}"
access_secret = "${CONFLUENCE_SECRET}"
consumer_key = "${CONFLUENCE_CONSUMER_KEY:-rw}"  # with default value

[diagrams]
kroki_url = "${KROKI_URL:-https://kroki.io}"
```

Supported syntax:

- `${VAR}` -- expands to the value of `VAR`, errors if unset
- `${VAR:-default}` -- expands to `VAR` if set, otherwise uses `default`

Expandable fields: `server.host`, `confluence.base_url`, `confluence.access_token`,
`confluence.access_secret`, `confluence.consumer_key`, `diagrams.kroki_url`.

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

## README.md as Homepage

If your `docs/` directory doesn't have an `index.md`, RW automatically uses `README.md` from the project root as the homepage. No configuration needed.

- `docs/index.md` exists: used as homepage (normal behavior)
- `docs/index.md` missing + `README.md` exists: README.md serves as homepage
- Live reload works for README.md changes too
