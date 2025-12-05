//! Kroki diagram rendering with parallel HTTP requests.

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

/// Error during diagram rendering.
#[derive(Debug)]
pub enum RenderError {
    Http { index: usize, message: String },
    Io { index: usize, message: String },
    InvalidPng { index: usize },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Http { index, message } => {
                write!(f, "HTTP error for diagram {}: {}", index, message)
            }
            RenderError::Io { index, message } => {
                write!(f, "IO error for diagram {}: {}", index, message)
            }
            RenderError::InvalidPng { index } => {
                write!(f, "Invalid PNG data for diagram {}", index)
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
) -> Result<RenderedDiagram, RenderError> {
    let url = format!("{}/plantuml/png", server_url);

    let response = agent
        .post(&url)
        .header("Content-Type", "text/plain")
        .send(diagram.source.as_bytes())
        .map_err(|e| RenderError::Http {
            index: diagram.index,
            message: e.to_string(),
        })?;

    let data = response
        .into_body()
        .read_to_vec()
        .map_err(|e| RenderError::Io {
            index: diagram.index,
            message: e.to_string(),
        })?;

    let (width, height) = get_png_dimensions(&data).ok_or(RenderError::InvalidPng {
        index: diagram.index,
    })?;

    let hash = diagram_hash("plantuml", &diagram.source);
    let filename = format!("diagram_{}.png", hash);
    let filepath = output_dir.join(&filename);

    std::fs::write(&filepath, &data).map_err(|e| RenderError::Io {
        index: diagram.index,
        message: e.to_string(),
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
/// * `server_url` - Kroki server URL (e.g., "https://kroki.io")
/// * `output_dir` - Directory to save rendered PNG files
/// * `pool_size` - Number of parallel threads
///
/// # Returns
/// Vector of rendered diagram info, or first error encountered.
pub fn render_all(
    diagrams: Vec<DiagramRequest>,
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
            message: format!("Failed to create thread pool: {}", e),
        })?;

    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();

    pool.install(|| {
        diagrams
            .par_iter()
            .map(|d| render_one(&agent, d, server_url, output_dir))
            .collect()
    })
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
