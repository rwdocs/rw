# TASKS

## Current

Move diagram rendering from Python to Rust. In Python create tempdir and pass
to Rust.

## Completed

### Move diagram rendering from Python to Rust

**Task**: Move Kroki diagram rendering from Python (`kroki.py`) to Rust with
N-sized connection pool, saving rendered diagrams to a provided directory.

**Research Summary** (2025-12-04):

#### Recommended Libraries

| Purpose | Library | Rationale |
|---------|---------|-----------|
| HTTP Client | **ureq** | Sync, minimal deps, no async runtime, small binary |
| Thread Pool | **rayon** | Simple parallel iterators, configurable pool size |

#### Architecture Proposal

```rust
// New dependencies in Cargo.toml
ureq = { version = "3", features = ["rustls"] }
rayon = "1.10"
```

```rust
// kroki.rs - New module
use rayon::prelude::*;
use std::path::PathBuf;

pub struct RenderedDiagram {
    pub index: usize,
    pub filename: String,
    pub width: u32,
    pub height: u32,
}

pub fn render_all(
    diagrams: Vec<DiagramInfo>,
    server_url: &str,
    output_dir: &Path,
    pool_size: usize,
) -> Result<Vec<RenderedDiagram>, Error> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(pool_size)
        .build()?;

    pool.install(|| {
        diagrams.par_iter()
            .map(|d| render_one(d, server_url, output_dir))
            .collect()
    })
}

fn render_one(
    diagram: &DiagramInfo,
    server_url: &str,
    output_dir: &Path,
) -> Result<RenderedDiagram, Error> {
    let url = format!("{}/plantuml/png", server_url);
    let response = ureq::post(&url)
        .header("Content-Type", "text/plain")
        .send(diagram.source.as_bytes())?;

    let data = response.body_mut().read_to_vec()?;
    let (width, height) = get_png_dimensions(&data);
    let filename = format!("{}.png", diagram.hash);
    std::fs::write(output_dir.join(&filename), &data)?;

    Ok(RenderedDiagram { index: diagram.index, filename, width, height })
}
```

#### PyO3 Integration

Simple blocking call with GIL release:

```rust
#[pyfunction]
fn render_diagrams(
    py: Python<'_>,
    diagrams: Vec<PyDiagramInfo>,
    server_url: &str,
    output_dir: PathBuf,
    pool_size: usize,
) -> PyResult<Vec<PyRenderedDiagram>> {
    py.allow_threads(|| {
        render_all(diagrams, server_url, &output_dir, pool_size)
    })
}
```

#### Key Benefits

1. **Simple**: No async complexity, just parallel threads
2. **Fast compile**: ureq + rayon have minimal dependencies
3. **Small binary**: No tokio/hyper overhead
4. **Easy PyO3**: Just release GIL with `py.allow_threads()`

#### Sources

- [ureq](https://github.com/algesten/ureq) - Minimal sync HTTP client
- [rayon](https://github.com/rayon-rs/rayon) - Data parallelism library
- [LogRocket: Rust HTTP clients](https://blog.logrocket.com/best-rust-http-client/)
