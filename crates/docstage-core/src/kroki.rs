//! Kroki diagram rendering with parallel HTTP requests.
//!
//! This module handles parallel diagram rendering via the Kroki service:
//! - Renders `PlantUML` diagrams to PNG via HTTP POST
//! - Uses rayon thread pool for parallel requests
//! - Extracts PNG dimensions for display width calculation
//! - Generates content-based filenames via SHA256 hashing

use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use ureq::Agent;

/// Result of rendering a single diagram.
#[derive(Debug, Clone)]
pub struct RenderedDiagram {
    pub index: usize,
    pub filename: String,
    pub width: u32,
    pub height: u32,
}

/// Diagram info for rendering.
#[derive(Debug, Clone)]
pub struct DiagramRequest {
    pub index: usize,
    pub source: String,
}

/// Single diagram rendering error.
#[derive(Debug, Clone)]
pub struct DiagramError {
    pub index: usize,
    pub kind: DiagramErrorKind,
}

/// Kind of diagram rendering error.
#[derive(Debug, Clone)]
pub enum DiagramErrorKind {
    Http(String),
    Io(String),
    InvalidPng,
}

impl std::fmt::Display for DiagramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            DiagramErrorKind::Http(msg) => {
                write!(f, "diagram {}: HTTP error: {msg}", self.index)
            }
            DiagramErrorKind::Io(msg) => {
                write!(f, "diagram {}: IO error: {msg}", self.index)
            }
            DiagramErrorKind::InvalidPng => {
                write!(f, "diagram {}: invalid PNG data", self.index)
            }
        }
    }
}

/// Error during diagram rendering (may contain multiple errors).
#[derive(Debug)]
pub enum RenderError {
    /// Single diagram error (legacy, for backwards compatibility).
    Http {
        index: usize,
        message: String,
    },
    Io {
        index: usize,
        message: String,
    },
    InvalidPng {
        index: usize,
    },
    /// Multiple diagram errors collected during parallel rendering.
    Multiple(Vec<DiagramError>),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Http { index, message } => {
                write!(f, "HTTP error for diagram {index}: {message}")
            }
            RenderError::Io { index, message } => {
                write!(f, "IO error for diagram {index}: {message}")
            }
            RenderError::InvalidPng { index } => {
                write!(f, "Invalid PNG data for diagram {index}")
            }
            RenderError::Multiple(errors) => {
                writeln!(f, "{} diagram(s) failed to render:", errors.len())?;
                for error in errors {
                    writeln!(f, "  - {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for RenderError {}

/// Extract width and height from PNG image data.
///
/// PNG format: 8-byte signature, then IHDR chunk with width/height at bytes 16-24.
fn get_png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 {
        return None;
    }

    // PNG signature check
    if &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    // IHDR chunk: width at bytes 16-20, height at bytes 20-24 (big-endian)
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((width, height))
}

/// Generate SHA256 hash prefix for diagram filename.
fn diagram_hash(diagram_type: &str, source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(diagram_type.as_bytes());
    hasher.update(b":");
    hasher.update(source.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..6])
}

/// Render a single diagram via Kroki.
fn render_one(
    agent: &Agent,
    diagram: &DiagramRequest,
    server_url: &str,
    output_dir: &Path,
) -> Result<RenderedDiagram, DiagramError> {
    let url = format!("{server_url}/plantuml/png");

    let response = agent
        .post(&url)
        .header("Content-Type", "text/plain")
        .send(diagram.source.as_bytes())
        .map_err(|e| DiagramError {
            index: diagram.index,
            kind: DiagramErrorKind::Http(e.to_string()),
        })?;

    let data = response
        .into_body()
        .read_to_vec()
        .map_err(|e| DiagramError {
            index: diagram.index,
            kind: DiagramErrorKind::Io(e.to_string()),
        })?;

    let (width, height) = get_png_dimensions(&data).ok_or(DiagramError {
        index: diagram.index,
        kind: DiagramErrorKind::InvalidPng,
    })?;

    let hash = diagram_hash("plantuml", &diagram.source);
    let filename = format!("diagram_{hash}.png");
    let filepath = output_dir.join(&filename);

    std::fs::write(&filepath, &data).map_err(|e| DiagramError {
        index: diagram.index,
        kind: DiagramErrorKind::Io(e.to_string()),
    })?;

    Ok(RenderedDiagram {
        index: diagram.index,
        filename,
        width,
        height,
    })
}

/// Render all diagrams in parallel using Kroki service.
///
/// # Arguments
/// * `diagrams` - List of diagrams to render
/// * `server_url` - Kroki server URL (e.g., `<https://kroki.io>`)
/// * `output_dir` - Directory to save rendered PNG files
/// * `pool_size` - Number of parallel threads
///
/// # Returns
/// Vector of rendered diagram info, or all errors collected during rendering.
///
/// # Errors
///
/// Returns `RenderError::Multiple` if any diagrams fail to render,
/// containing all errors (not just the first one).
pub fn render_all(
    diagrams: &[DiagramRequest],
    server_url: &str,
    output_dir: &Path,
    pool_size: usize,
) -> Result<Vec<RenderedDiagram>, RenderError> {
    if diagrams.is_empty() {
        return Ok(Vec::new());
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(pool_size)
        .build()
        .map_err(|e| RenderError::Io {
            index: 0,
            message: format!("Failed to create thread pool: {e}"),
        })?;

    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();

    let results: Vec<Result<RenderedDiagram, DiagramError>> = pool.install(|| {
        diagrams
            .par_iter()
            .map(|d| render_one(&agent, d, server_url, output_dir))
            .collect()
    });

    // Partition into successes and failures
    let mut rendered = Vec::with_capacity(diagrams.len());
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok(diagram) => rendered.push(diagram),
            Err(error) => errors.push(error),
        }
    }

    if errors.is_empty() {
        Ok(rendered)
    } else {
        Err(RenderError::Multiple(errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_png_dimensions() {
        // Minimal valid PNG with 100x50 dimensions
        let mut png_data = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR length
            b'I', b'H', b'D', b'R', // IHDR type
            0x00, 0x00, 0x00, 0x64, // width = 100
            0x00, 0x00, 0x00, 0x32, // height = 50
        ];
        png_data.extend_from_slice(&[0; 5]); // bit depth, color type, etc.

        let dims = get_png_dimensions(&png_data);
        assert_eq!(dims, Some((100, 50)));
    }

    #[test]
    fn test_get_png_dimensions_invalid() {
        let invalid_data = b"not a png";
        assert_eq!(get_png_dimensions(invalid_data), None);
    }

    #[test]
    fn test_diagram_hash() {
        let hash1 = diagram_hash("plantuml", "@startuml\nA -> B\n@enduml");
        let hash2 = diagram_hash("plantuml", "@startuml\nA -> B\n@enduml");
        let hash3 = diagram_hash("plantuml", "@startuml\nC -> D\n@enduml");

        assert_eq!(hash1.len(), 12);
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
