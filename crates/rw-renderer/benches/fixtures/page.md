---
title: Configuring the widget
description: Options, examples, and the processing pipeline.
---

# Configuring the widget

The **widget** transforms data through a small, predictable pipeline. The
current release is :status[Stable]{color=green} and the next one is
:status[In Progress]{color=yellow}. For the full option list run `widget --help`,
or see the [reference](reference.md).

## Install

:::tab[macOS]

Install with Homebrew: `brew install widget`.

:::tab[Linux]

Install with apt: `apt install widget`.

:::tab[Windows]

Install with winget: `winget install widget`.

:::

## Options

The most common options and their defaults:

| Option      | Type    | Default    | Description                           |
| ----------- | ------- | ---------- | ------------------------------------- |
| `--input`   | path    | *(stdin)*  | File to read input from.              |
| `--output`  | path    | *(stdout)* | File to write the result to.          |
| `--format`  | string  | `json`     | Output format: `json`, `yaml`, `csv`. |
| `--jobs`    | integer | `4`        | Number of parallel workers.           |

## Examples

Read a file and write JSON, using eight workers:

```bash
widget --input data.txt --output result.json --format json --jobs 8
```

The same thing through the library API:

```rust
use widget::{Widget, Format};

fn main() -> anyhow::Result<()> {
    let widget = Widget::builder().format(Format::Json).jobs(8).build()?;
    let result = widget.run("data.txt")?;
    println!("{}", result.summary());
    Ok(())
}
```

> [!NOTE]
> Changing `--format` after the first run invalidates the cache, so the next
> run is a full rebuild rather than an incremental one.

## Pipeline

Every run flows through three stages:

1. **Parse** — read and validate input; malformed records are collected, not fatal.
2. **Transform** — apply each configured rule to every record.
3. **Emit** — serialize to the requested format.

## Next steps

- Read the [reference](reference.md) for every option.
- Skim the [examples](examples.md) for common setups.
- Report anything surprising on the issue tracker.
