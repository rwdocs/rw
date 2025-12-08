# RD-005: Configuration File

## Problem Statement

Docstage currently has two separate configuration mechanisms:

1. **CLI options** for the `serve` command: `--source-dir`, `--cache-dir`, `--host`,
   `--port`, `--kroki-url`

2. **config.toml** for Confluence credentials: `base_url`, `access_token`,
   `access_secret`, `consumer_key`

This creates several problems:

- **Inconsistent configuration**: Some settings are in TOML, others are CLI-only
- **Growing CLI complexity**: Each new feature adds more flags (e.g., `--kroki-url`,
  future `--include-dirs`)
- **No project-level defaults**: Users must specify the same options repeatedly
- **Missing features**: PlantUML include directories cannot be configured for the
  `serve` command

## Requirements

### Functional Requirements

1. **Unified configuration file** (`docstage.toml`) that consolidates all settings:
    - Server settings (host, port)
    - Documentation settings (source directory, cache directory)
    - Diagram rendering settings (Kroki URL, include directories, config file, DPI)
    - Confluence settings (moved from `config.toml`)

2. **Auto-discovery**: Look for `docstage.toml` in current directory and parent
   directories (similar to `pyproject.toml`, `.gitignore`)

3. **CLI override**: CLI options override config file values when specified

### Non-Functional Requirements

1. **Simple format**: Use TOML for consistency with Python ecosystem
2. **Sensible defaults**: Work out of the box with minimal configuration
3. **Clear error messages**: Report missing or invalid configuration clearly

## Configuration Schema

```toml
# docstage.toml

[server]
host = "127.0.0.1"      # Default: 127.0.0.1
port = 8080             # Default: 8080

[docs]
source_dir = "docs"     # Default: docs
cache_dir = ".cache"    # Default: .cache

[diagrams]
kroki_url = "https://kroki.io"  # Optional, enables diagram rendering
include_dirs = ["."]           # Directories to search for !include files
config_file = "config.iuml"    # PlantUML config file name (searched in include_dirs)
dpi = 192                      # Default: 192 (retina)

[confluence]
base_url = "https://confluence.example.com"
access_token = "..."
access_secret = "..."
consumer_key = "docstage"  # Default: docstage

[confluence.test]
space_key = "TEST"  # For test commands
```

## Implementation Plan

### Phase 1: Configuration Module

1. **Rewrite `config.py`** with new `docstage.toml` schema:
    - Define dataclasses for each section
    - Implement TOML parsing with validation
    - Implement config file discovery (walk up directory tree)

2. **Remove `config.toml` support**: Replace entirely with `docstage.toml`

### Phase 2: CLI Integration

1. **Update `serve` command**:
    - Load config file first
    - Apply CLI overrides on top
    - Remove defaults from CLI options (use config defaults instead)

2. **Update other commands** (`convert`, `create`, `update`, etc.):
    - Use unified config loading
    - Support `--config` flag to specify alternative config file path

### Phase 3: Diagram Configuration

1. **Pass diagram settings to `PageRenderer`**:
    - `kroki_url` from config
    - `include_dirs` from config (relative to config file location)
    - `config_file` and `dpi` for PlantUML preprocessing

2. **Update `MarkdownConverter` initialization** in renderer

### Phase 4: Documentation and Migration

1. **Create `docstage.toml.example`** with all options documented
2. **Update README.md** with configuration instructions
3. **Add migration guide** from `config.toml` to `docstage.toml`

## File Changes

### New Files

- `packages/docstage/src/docstage/config.py` - Rewrite with new schema

### Modified Files

- `packages/docstage/src/docstage/cli.py` - Config loading, CLI override logic
- `packages/docstage/src/docstage/server.py` - Use config object
- `packages/docstage/src/docstage/core/renderer.py` - Accept diagram config
- `docstage.toml.example` - New example file
- `README.md` - Configuration documentation
- `CLAUDE.md` - Update development commands if needed

## Example Usage

### Minimal Configuration

```toml
# docstage.toml
[docs]
source_dir = "docs"

[diagrams]
kroki_url = "https://kroki.io"
```

```bash
# Just run serve - uses config file
docstage serve
```

### Full Configuration

```toml
# docstage.toml
[server]
host = "0.0.0.0"
port = 3000

[docs]
source_dir = "documentation"
cache_dir = ".docstage-cache"

[diagrams]
kroki_url = "https://kroki.cian.tech"
include_dirs = [".", "includes", "gen/includes"]
config_file = "config.iuml"
dpi = 192

[confluence]
base_url = "https://confluence.example.com"
access_token = "xxx"
access_secret = "yyy"
consumer_key = "docstage"
```

### CLI Override

```bash
# Override port from config
docstage serve --port 9000

# Use different config file
docstage serve --config /path/to/docstage.toml
```

## Testing Strategy

1. **Unit tests** for config parsing and validation
2. **Unit tests** for config file discovery
3. **Integration tests** for CLI override behavior
