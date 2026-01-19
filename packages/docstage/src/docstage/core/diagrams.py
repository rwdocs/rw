"""Diagram rendering support.

NOTE: Diagram rendering logic has been moved to Rust (docstage-diagrams crate).
This module is kept for backward compatibility but rendering is now handled by
the Rust `DiagramProcessor.post_process()` method with caching support.

The Rust implementation provides:
- Parallel rendering via rayon
- Content-based caching via DiagramCache trait
- SVG scaling based on DPI
- Google Fonts stripping
- Placeholder replacement

See RD-009 for migration details.
"""
