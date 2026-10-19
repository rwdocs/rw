---
paths:
  - "crates/rw-meta/**"
  - "crates/rw-storage*/**"
  - "crates/rw-site/**"
---

# Page metadata and storage

- `SiteStateBuilder::add_document` panics on a `namespace` or `name` outside
  the `Namespace` grammar, so every storage must hand over validated `Meta`.
  `Meta::resolve` validates; `Document`'s deserializer does not, which is why
  the S3 reader re-validates manifest documents. An invalid `kind` does not
  panic but yields a section ref that does not parse back.
- `Meta.attrs` is in no render fingerprint. A renderer that starts reading
  attrs must add them to its cache dependencies. Only NAPI
  `renderPage().meta.attrs` exposes them.
- The serialized `Document` is the S3 manifest wire format, still version 1.
  New fields must be optional and omitted when empty. Older readers drop
  unknown fields, so upgrade S3 readers before publishers rely on a new field.
