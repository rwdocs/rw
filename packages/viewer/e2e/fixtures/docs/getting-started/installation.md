# Installation

Learn how to install the platform on your system.

## Requirements

Before installing, ensure you have:

- Node.js 18 or later
- Python 3.14 or later
- Rust 1.91 or later

## Install via npm

```bash
npm install -g platform-cli
```

## Install via pip

```bash
pip install platform
```

## Verify Installation

After installation, verify it works:

```bash
platform --version
```

You should see the version number printed.

## Troubleshooting

If you encounter issues, check the [Configuration Guide](./configuration.md).

## Platform Support

The platform supports the following operating systems and architectures:

| OS      | Architecture          | Status              |
| ------- | --------------------- | ------------------- |
| macOS   | arm64 (Apple Silicon) | Fully supported     |
| macOS   | x86_64 (Intel)        | Fully supported     |
| Linux   | x86_64                | Fully supported     |
| Linux   | arm64                 | Fully supported     |
| Windows | x86_64                | Fully supported     |
| FreeBSD | x86_64                | Community supported |

## Environment Variables

The following environment variables can be used to configure the platform:

| Variable             | Description            | Default                   |
| -------------------- | ---------------------- | ------------------------- |
| `PLATFORM_HOME`      | Installation directory | `~/.platform`             |
| `PLATFORM_CONFIG`    | Config file path       | `~/.platform/config.toml` |
| `PLATFORM_LOG_LEVEL` | Logging verbosity      | `info`                    |
| `PLATFORM_CACHE_DIR` | Cache directory        | `~/.platform/cache`       |
| `PLATFORM_DATA_DIR`  | Data directory         | `~/.platform/data`        |
| `PLATFORM_TEMP_DIR`  | Temporary directory    | System default            |

## Upgrading

To upgrade to the latest version:

```bash
npm update -g platform-cli
```

Or if installed via pip:

```bash
pip install --upgrade platform
```

After upgrading, verify the new version:

```bash
platform --version
```

## Uninstalling

To remove the platform:

```bash
npm uninstall -g platform-cli
```

Then clean up the data directory:

```bash
rm -rf ~/.platform
```
