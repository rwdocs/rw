# RD-004: Diagram Rendering

## Overview

Enable diagram rendering (PlantUML, Mermaid, and other Kroki-supported formats) in
the Docstage frontend. Currently, diagrams are rendered as syntax-highlighted code
blocks instead of visual images.

**Tagline:** "Where documentation takes the stage"

## Problem Statement

The Docstage backend has two rendering modes:

1. **Confluence mode** (`convert()`) - Extracts PlantUML blocks, renders them via Kroki
   to PNG images, uploads as Confluence attachments.

2. **HTML mode** (`convert_html()`) - Renders PlantUML blocks as syntax-highlighted
   `<pre><code class="language-plantuml">` elements.

The frontend uses HTML mode, so diagrams appear as raw source code instead of rendered
images. This defeats the purpose of using diagram languages like PlantUML.

**Example current output:**

```html
<pre><code class="language-plantuml">@startuml
Alice -> Bob: Hello
Bob -> Alice: Hi
@enduml</code></pre>
```

**Expected output:**

An `<img>` element displaying the rendered diagram.

## Goals

1. Render PlantUML diagrams as images in HTML output.
2. Support all Kroki-supported diagram types (Mermaid, GraphViz, etc.).
3. Cache rendered diagrams to avoid re-rendering on every request.
4. Maintain fast page load times (diagram rendering should not block page display).

## Non-Goals (This RD)

- Client-side diagram rendering (e.g., PlantUML.js, Mermaid.js in browser).
- Diagram source editing in the frontend.
- Custom diagram styling or themes.
- Diagram export functionality.

## Architecture

### High-Level Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Markdown Source                                                             │
│                                                                             │
│  ```plantuml                                                                │
│  @startuml                                                                  │
│  Alice -> Bob: Hello                                                        │
│  @enduml                                                                    │
│  ```                                                                        │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Rust Core (convert_html)                                                   │
│                                                                             │
│  1. PlantUmlFilter extracts diagram blocks                                  │
│  2. Kroki client renders to PNG/SVG                                         │
│  3. HTML renderer outputs <img> tags with data URIs or file paths           │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Python Backend                                                              │
│                                                                             │
│  - Caches rendered HTML (includes diagram references)                       │
│  - Serves diagram images via /api/diagrams/{hash}.svg or data URIs          │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Frontend                                                                    │
│                                                                             │
│  - Displays <img> elements within prose content                             │
│  - No special handling needed (standard HTML images)                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Component Changes

#### Rust Core (docstage-core)

**Extend `HtmlRenderer` to support diagram rendering:**

The key architectural decision is **where** diagram data lives in the output.

**Option A: Data URIs (inline SVG/PNG as base64)**

Pros:

- Self-contained HTML output
- No additional API endpoints needed
- Simpler caching (HTML includes everything)

Cons:

- Larger HTML payloads
- Cannot cache diagrams separately from page content
- Base64 encoding adds ~33% size overhead for PNG

**Option B: External diagram files with API endpoint**

Pros:

- Smaller HTML payloads
- Diagrams cached independently
- Browser can cache diagram images separately

Cons:

- Additional API endpoint complexity
- Two round-trips to display page with diagrams

**Recommendation: Option A (Inline SVG)**

SVG is text-based, compresses well, scales perfectly, and can be inlined without
base64 encoding. The slight increase in HTML size is acceptable for documentation
use cases and simplifies the architecture significantly.

**Critical reason for inline SVG**: Some diagram types (notably C4) contain clickable
links (`<a xlink:href="...">`). SVG links only work when the SVG is part of the DOM.
Loading SVG via `<img src="...">` disables all interactivity for security reasons.
Using `<object>` or `<iframe>` would restore interactivity but introduces cross-origin
issues and complicates sizing/styling. Inline SVG is the only approach that preserves
link functionality reliably.

### Per-Diagram Configuration

Users can override the default rendering behavior using attributes in the code fence
info string. Attributes are space-separated key=value pairs after the language name:

```markdown
```plantuml
Default: inline SVG (interactive, links work)
```

```plantuml format=png
Force PNG output (smaller for complex diagrams, no interactivity)
```

```plantuml format=img
External SVG via <img> tag (cacheable separately, but no links)
```
```

**Supported attributes:**

| Attribute | Values | Default | Description |
|-----------|--------|---------|-------------|
| `format`  | `svg`, `png`, `img` | `svg` | Output format. `svg` = inline SVG, `png` = inline PNG (base64), `img` = external SVG via `<img>` tag |

**Parsing rules:**

- First word is the diagram language (e.g., `plantuml`, `mermaid`)
- Remaining words are parsed as attributes
- Attributes use `key=value` syntax (no spaces around `=`)
- Unknown attributes are ignored
- This follows CommonMark spec which allows arbitrary text after language

**Example parsing:**

```
"plantuml format=png" → language: "plantuml", format: "png"
"mermaid"             → language: "mermaid", format: "svg" (default)
"c4plantuml format=svg" → language: "c4plantuml", format: "svg"
```

### Kroki Integration

Kroki supports multiple diagram types via language identifiers:

| Language      | Kroki Endpoint     |
|---------------|-------------------|
| `plantuml`    | `/plantuml/svg`   |
| `mermaid`     | `/mermaid/svg`    |
| `graphviz`    | `/graphviz/svg`   |
| `ditaa`       | `/ditaa/svg`      |
| `blockdiag`   | `/blockdiag/svg`  |
| `c4plantuml`  | `/c4plantuml/svg` |

The existing `PlantUmlFilter` should be generalized to `DiagramFilter` to extract
any supported diagram type.

### SVG vs PNG

**Choose SVG for HTML output:**

- Vector graphics scale to any resolution
- Text remains selectable/searchable
- Smaller file size for most diagrams
- No DPI configuration needed
- Can be inlined directly in HTML (no base64)

**Keep PNG for Confluence output:**

- Confluence has better PNG support
- Existing attachment workflow uses PNG
- Retina display scaling already implemented

## Implementation Plan

### Phase 1: Rust Core - Generalize Diagram Extraction

1. **Rename `PlantUmlFilter` to `DiagramFilter`**
    - Support multiple diagram languages (plantuml, mermaid, graphviz, etc.)
    - Extract `ExtractedDiagram` with `language` field

2. **Update `ExtractedDiagram` struct:**

    ```rust
    pub struct ExtractedDiagram {
        pub source: String,
        pub index: usize,
        pub language: DiagramLanguage,
        pub format: DiagramFormat,  // From info string attributes
    }

    pub enum DiagramLanguage {
        PlantUml,
        Mermaid,
        GraphViz,
        Ditaa,
        BlockDiag,
        C4PlantUml,
    }

    #[derive(Default)]
    pub enum DiagramFormat {
        #[default]
        Svg,      // Inline SVG (default, supports links)
        Png,      // Inline PNG as base64 data URI
        Img,      // External SVG via <img> tag
    }
    ```

3. **Add info string parser:**

    ```rust
    /// Parse code fence info string into language and attributes
    /// Example: "plantuml format=png" → (PlantUml, {format: Png})
    fn parse_info_string(info: &str) -> Option<(DiagramLanguage, DiagramFormat)> {
        let mut parts = info.split_whitespace();
        let language = DiagramLanguage::from_str(parts.next()?)?;

        let mut format = DiagramFormat::default();
        for part in parts {
            if let Some((key, value)) = part.split_once('=') {
                if key == "format" {
                    format = match value {
                        "png" => DiagramFormat::Png,
                        "img" => DiagramFormat::Img,
                        _ => DiagramFormat::Svg,
                    };
                }
            }
        }

        Some((language, format))
    }
    ```

4. **Add `DiagramLanguage::kroki_endpoint()` method:**

    ```rust
    impl DiagramLanguage {
        pub fn kroki_endpoint(&self) -> &'static str {
            match self {
                Self::PlantUml => "plantuml",
                Self::Mermaid => "mermaid",
                Self::GraphViz => "graphviz",
                // ...
            }
        }
    }
    ```

### Phase 2: Rust Core - SVG Rendering via Kroki

1. **Add SVG output support to `kroki.rs`:**

    ```rust
    pub enum OutputFormat {
        Png,
        Svg,
    }

    pub fn render_all_svg(
        diagrams: &[DiagramRequest],
        server_url: &str,
    ) -> Result<Vec<RenderedDiagram>, RenderError>
    ```

2. **Update `RenderedDiagram` for SVG:**

    ```rust
    pub struct RenderedDiagram {
        pub index: usize,
        pub content: DiagramContent,
    }

    pub enum DiagramContent {
        Png { filename: String, width: u32, height: u32 },
        Svg { data: String },  // Raw SVG string
    }
    ```

3. **Handle SVG response from Kroki:**

    - Request: POST to `{server_url}/{language}/svg`
    - Response: SVG XML string
    - No dimension extraction needed (SVG is scalable)

### Phase 3: Rust Core - HTML Renderer Integration

1. **Add `convert_html_with_diagrams()` method to `MarkdownConverter`:**

    ```rust
    pub fn convert_html_with_diagrams(
        &self,
        markdown: &str,
        kroki_url: &str,
    ) -> Result<HtmlConvertResult, ConvertError>
    ```

2. **Update `HtmlRenderer` to handle diagram placeholders:**

    - During rendering: emit `{{DIAGRAM_N}}` placeholders (like Confluence)
    - After Kroki rendering: replace based on `DiagramFormat`

3. **Output strategy based on format:**

    **`format=svg` (default)** - Inline SVG:

    ```html
    <figure class="diagram">
        <svg xmlns="http://www.w3.org/2000/svg" ...>
            <!-- Kroki SVG content -->
        </svg>
    </figure>
    ```

    **`format=png`** - Inline PNG as base64 data URI:

    ```html
    <figure class="diagram">
        <img src="data:image/png;base64,iVBORw0KGgo..." alt="diagram">
    </figure>
    ```

    **`format=img`** - External SVG via img tag:

    ```html
    <figure class="diagram">
        <img src="/api/diagrams/{hash}.svg" alt="diagram">
    </figure>
    ```

    All wrapped in `<figure>` for semantic HTML and styling hooks.

### Phase 4: Python Backend - Integration

1. **Update `PageRenderer` to use diagram-enabled conversion:**

    ```python
    class PageRenderer:
        def __init__(self, kroki_url: str | None = None):
            self.kroki_url = kroki_url

        def render(self, source_path: Path) -> RenderResult:
            if self.kroki_url:
                return self.converter.convert_html_with_diagrams(
                    content, self.kroki_url
                )
            return self.converter.convert_html(content)
    ```

2. **Add Kroki URL to configuration:**

    ```toml
    # docstage.toml
    [rendering]
    kroki_url = "https://kroki.io"  # or self-hosted instance
    ```

3. **Update cache invalidation:**

    - Diagrams are embedded in HTML, so existing mtime-based invalidation works
    - For `format=img`, diagrams stored in `.cache/diagrams/{hash}.svg`

4. **Add diagram endpoint for `format=img`:**

    ```python
    # GET /api/diagrams/{hash}.svg
    async def get_diagram(request: web.Request) -> web.Response:
        hash = request.match_info["hash"]
        diagram_path = cache_dir / "diagrams" / f"{hash}.svg"
        if not diagram_path.exists():
            raise web.HTTPNotFound()
        return web.FileResponse(
            diagram_path,
            headers={
                "Content-Type": "image/svg+xml",
                "Cache-Control": "public, max-age=31536000, immutable",
            }
        )
    ```

    Content-addressed filenames allow aggressive caching (1 year, immutable).

### Phase 5: Frontend - Styling

1. **Add diagram styles to Tailwind:**

    ```css
    /* In app.css or as Tailwind plugin */
    .prose figure.diagram {
        @apply my-6 flex justify-center;
    }

    .prose figure.diagram svg {
        @apply max-w-full h-auto;
    }
    ```

2. **Ensure SVG responsiveness:**

    - SVG should scale within content area
    - Preserve aspect ratio
    - Center alignment for diagrams

### Phase 6: Testing & Documentation

1. **Add Rust tests:**
    - DiagramFilter extraction for multiple languages
    - SVG rendering via Kroki
    - Placeholder replacement

2. **Add Python tests:**
    - PageRenderer with Kroki integration
    - Configuration parsing

3. **Add integration tests:**
    - End-to-end: markdown with diagram → rendered HTML with SVG

4. **Update CLAUDE.md:**
    - Document Kroki URL configuration
    - Add supported diagram types

## Technical Decisions

### Why Server-Side Rendering (Kroki) over Client-Side (Mermaid.js)?

1. **Consistency** - Same rendering engine for all output formats (Confluence, HTML).
2. **Performance** - Diagrams rendered once, cached, not on every page view.
3. **Bundle size** - No need to ship diagram libraries to browser.
4. **Compatibility** - Kroki supports 20+ diagram types with one integration.

### Why SVG over PNG for HTML?

1. **Scalability** - Perfect rendering at any zoom level or screen density.
2. **Size** - Text-based SVG often smaller than raster PNG.
3. **Accessibility** - Text in SVG remains selectable.
4. **Simplicity** - No need for separate image files or DPI calculations.

### Why Inline SVG over External Files?

1. **Single request** - Page content includes everything needed.
2. **Caching simplicity** - HTML cache includes diagrams.
3. **No CORS issues** - SVG is part of the document.
4. **CSP friendly** - No external image requests.

### Error Handling

When Kroki rendering fails:

1. **Fallback to code block** - Display original source as syntax-highlighted code.
2. **Add error indicator** - CSS class `diagram-error` for styling.
3. **Include error message** - As HTML comment or data attribute.

```html
<figure class="diagram diagram-error" data-error="Kroki server unreachable">
    <pre><code class="language-plantuml">@startuml
Alice -> Bob: Hello
@enduml</code></pre>
</figure>
```

## Configuration

### New Configuration Options

```toml
# docstage.toml
[rendering]
# Kroki server URL for diagram rendering
# Set to null/omit to disable diagram rendering (show as code blocks)
kroki_url = "https://kroki.io"

# Diagram output format for HTML (svg recommended)
# diagram_format = "svg"  # Future: could support "png" with data URIs
```

### Environment Variables

```bash
# Override Kroki URL via environment
DOCSTAGE_KROKI_URL=http://localhost:8000
```

## Dependencies

### Rust (existing)

- `ureq` - HTTP client for Kroki requests (already used)
- `rayon` - Parallel diagram rendering (already used)

### No New Dependencies

This implementation reuses existing infrastructure.

## Success Metrics

1. **Diagram render time:** < 500ms per diagram (Kroki latency).
2. **Page load time:** No regression for pages without diagrams.
3. **Cache efficiency:** Diagrams re-rendered only when source changes.

## Migration

### Backward Compatibility

- Existing HTML output (code blocks) continues to work.
- Diagram rendering is opt-in via `kroki_url` configuration.
- No breaking changes to API responses.

### Upgrade Path

1. Deploy new version with `kroki_url` unset (existing behavior).
2. Configure Kroki URL when ready to enable diagrams.
3. Clear cache to regenerate HTML with embedded diagrams.

## Future Extensions

These features are explicitly deferred:

1. **Client-side rendering** - Mermaid.js fallback when Kroki unavailable.
2. **Diagram themes** - Custom PlantUML/Mermaid themes.
3. **Diagram captions** - Figure captions from markdown syntax.
4. **Click-to-zoom** - Lightbox for large diagrams.
5. **PNG fallback** - For browsers with SVG issues.

## References

- [Kroki Documentation](https://kroki.io/)
- [PlantUML Language Reference](https://plantuml.com/)
- [Mermaid Documentation](https://mermaid.js.org/)
- [RD-001: Docstage Backend](RD-001-docstage-backend.md)
- [RD-002: Docstage Frontend](RD-002-docstage-frontend.md)
