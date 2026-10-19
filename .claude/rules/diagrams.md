---
paths:
  - "crates/rw-renderer/**"
  - "crates/rw-diagrams/**"
  - "crates/rw-kroki/**"
  - "crates/rw-confluence/**"
  - "packages/viewer/src/**"
  - "packages/viewer/e2e/**"
---

# Diagrams

- Diagram providers (`rw-kroki` is the only one) return content — bytes, a
  size, a digest — never markup; the `RenderBackend` decides markup. Keep
  `rw-kroki` and `rw-diagrams` independent of `rw-renderer`. The two-phase
  flow is described in `rw-renderer`'s crate docs.
- Kroki SVG ids are unique only within one diagram, so `HtmlBackend` wraps each
  inline SVG in `<rw-diagram>` and the viewer gives it a shadow root.
  `querySelector` from the page does not reach diagram internals; use
  `diagramSource` / `diagramShadowRoots` from
  `packages/viewer/src/lib/diagram/source.ts`. A hand-authored
  `<figure class="diagram">` has no wrapper and no isolation; both shapes must
  keep working.
