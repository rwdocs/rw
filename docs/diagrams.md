# Diagram Rendering

RW renders diagrams in fenced code blocks via [Kroki](https://kroki.io). Diagrams are written inline in markdown using fenced code blocks with the diagram language as the language identifier. Kroki processes the diagram source and returns rendered output that is embedded directly into the page.

## Supported Formats

RW supports PlantUML, Mermaid, GraphViz, and 14+ other formats supported by Kroki. Any diagram language supported by your Kroki instance can be used as the code block language identifier.

## Configuration

Add a `[diagrams]` section to your `rw.toml`:

```toml
[diagrams]
kroki_url = "https://kroki.io"  # Kroki server URL (required)
include_dirs = ["."]            # PlantUML !include search paths
dpi = 192                       # DPI for diagrams (default: 192, retina)
```

- **`kroki_url`** -- URL of the Kroki server. Required when the `[diagrams]` section is present.
- **`include_dirs`** -- Directories to search when resolving PlantUML `!include` directives.
- **`dpi`** -- DPI for rendered diagrams. The default of 192 produces retina-quality output.

### Without `rw.toml`

If your project has no `rw.toml`, set `RW_DIAGRAMS_KROKI_URL` in the environment instead:

```bash
export RW_DIAGRAMS_KROKI_URL="https://kroki.internal"
rw serve
```

This is the recommended setup when running RW across many repositories that share a single Kroki server -- you set the variable once (dev container, CI runner, dotfiles) and individual repositories need no config. An explicit `[diagrams] kroki_url` in a project's `rw.toml` still takes precedence over the environment variable.

## Usage

Use fenced code blocks with the diagram language as the identifier:

````markdown
```plantuml
Alice -> Bob: Hello
Bob -> Alice: Hi
```

```mermaid
graph LR
    A --> B
```
````

The diagram source is sent to the Kroki server, rendered, and embedded into the page as an SVG.

### Attributes

A diagram fence can carry an attribute block after the language, in braces:

````markdown
```mermaid {#architecture format=png}
graph LR
    A --> B
```
````

- **`format`** -- output format for this diagram, `svg` (default) or `png`. Set
  it inside the braces (`{format=png}`); there is no bare `format=png` form
  outside the braces.
- **`#id`** -- see [Diagram IDs](#diagram-ids) below.

## Diagram IDs

Every rendered diagram is wrapped in `<figure class="diagram">`, and that
`<figure>` carries a `data-diagram-id` attribute so host pages, tests, or
scripts can target a specific diagram.

Set `{#id}` on the fence to give a diagram a stable id:

````markdown
```mermaid {#architecture}
graph LR
    A --> B
```
````

renders as `<figure class="diagram" data-diagram-id="architecture">...</figure>`.

Diagrams without an explicit `{#id}` get an auto id of the form
`diagram-<n>`, where `<n>` is the zero-based index of the diagram among the
diagrams on the page (the first diagram is `diagram-0`, the second
`diagram-1`, and so on) -- not the position of its code block among all code
blocks. This means every diagram is addressable even if you never set an id.

The difference matters when diagrams move: an explicit id stays attached to
its diagram no matter where it ends up on the page, while an auto id is
positional and changes if you reorder or add diagrams before it. Set `{#id}`
on any diagram whose identity needs to stay stable (e.g. one referenced by an
external link or test).

This attribute is specific to the HTML output path; publishing to Confluence
does not emit it.

## Viewing diagrams

Complex diagrams can be hard to read at the width of the page column. Hover over any rendered diagram (or, on touch devices, look for the button in its top-right corner) and click **Expand diagram** to open it in a fullscreen popup.

In the popup you can:

- **Zoom** with the scroll wheel, a pinch gesture, or the on-screen `+` / `−` buttons.
- **Pan** by dragging the diagram once it is zoomed in.
- **Reset** to the initial fit with the reset button.

The diagram opens at its natural size, scaled down only when it is larger than the screen. Press `Escape` or click the close button to dismiss the popup.

## PlantUML Includes

PlantUML `!include` directives are resolved relative to the paths listed in `include_dirs`. This allows sharing common definitions, themes, and macros across multiple diagrams.

For example, with `include_dirs = ["common"]`, a diagram can reference shared definitions:

```plantuml
!include common-styles.iuml

Alice -> Bob: Hello
```

The file `common/common-styles.iuml` will be found and included before rendering.

## Rendering

Diagrams are rendered server-side via Kroki and embedded as SVGs. Key details:

- The default DPI of 192 produces retina-quality output suitable for high-resolution displays.
- PlantUML diagrams use the Roboto font by default (`skinparam defaultFontName Roboto`).
- Rendering is performed in parallel for pages with multiple diagrams.
- Rendered diagrams are cached to avoid redundant requests to the Kroki server.
