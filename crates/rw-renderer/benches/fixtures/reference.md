# Reference

Every command-line option, its type, and its default.

## Options

| Option      | Type    | Default    | Description                            |
| ----------- | ------- | ---------- | -------------------------------------- |
| `--input`   | path    | *(stdin)*  | File to read input from.               |
| `--output`  | path    | *(stdout)* | File to write the result to.           |
| `--format`  | string  | `json`     | Output format: `json`, `yaml`, `csv`.  |
| `--jobs`    | integer | `4`        | Number of parallel workers to run.     |
| `--verbose` | flag    | `false`    | Print progress information to stderr.  |

## Examples

Read from a file and write JSON to another:

```bash
widget --input data.txt --output result.json --format json --jobs 8
```

The equivalent using the library API:

```rust
use widget::{Widget, Format};

fn main() -> anyhow::Result<()> {
    let widget = Widget::builder()
        .format(Format::Json)
        .jobs(8)
        .build()?;
    let result = widget.run("data.txt")?;
    println!("{}", result.summary());
    Ok(())
}
```

Or from Python, where the same options are keyword arguments:

```python
from widget import Widget, Format

w = Widget(format=Format.JSON, jobs=8)
result = w.run("data.txt")
print(result.summary())
```

## Architecture

![Processing pipeline](images/pipeline.png)

The pipeline has three stages, run in order:

1. **Parse** — read and validate the input.
   - Malformed records are collected, not treated as fatal.
   - A running count is kept for the final summary.
2. **Transform** — apply each configured rule to every record.
3. **Emit** — serialize the transformed records to the requested format.

---

See the [getting started guide](article.md) for installation instructions.
