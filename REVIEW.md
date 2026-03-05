# Public API Review

## High Priority

| Crate | Issue |
|-------|-------|
| ~~**rw-server**~~ | ~~`ServerConfig.watch_patterns` removed~~ |
| ~~**rw-renderer**~~ | ~~`DirectiveContext` exposes all fields (`source_path`, `base_dir`, `line`, `read_file`) as pub — should use getters~~ |
| ~~**rw-renderer**~~ | ~~`DirectiveArgs` exposes mutable `content`, `id`, `classes` fields with no validation~~ |
| ~~**rw-renderer**~~ | ~~`escape_html()` is exported from the generic renderer but is HTML-specific — used by rw-confluence and rw-diagrams~~ |
| ~~**rw-confluence**~~ | ~~`Page` type from Confluence API response is exposed in `UpdateResult.page` — leaks full API structure to consumers~~ |
| ~~**rw-confluence**~~ | ~~`RequestToken`/`AccessToken` — actually used by CLI `generate_tokens` command, keep public~~ |

## Medium Priority

| Crate | Issue |
|-------|-------|
| **rw-server** | `run_server()` returns `Result<(), Box<dyn Error>>` — too generic, should define `ServerError` |
| **rw-confluence** | `RsaKeyError` variants expose PKCS#1 vs PKCS#8 implementation details |
| **rw-confluence** | `UpdateConfig` depends on `DiagramsConfig` from rw-config — tight cross-crate coupling |
| **rw-renderer** | `ExtractedCodeBlock` exposes mutable `attrs: HashMap<String, String>` |
| **rw-diagrams** | `RenderedDiagramInfo` exposes mutable pub fields (`filename`, `width`, `height`) |
| **rw-diagrams** | `DiagramTagGenerator` trait is heavy for what's essentially a callback |
| ~~**rw-storage-s3**~~ | ~~`page_bundle_key()` and `MANIFEST_KEY` made `pub(crate)`~~ |
| **rw-storage-s3** | Creates a dedicated tokio runtime per `S3Storage` instance |
| **rw-napi** | `create_site()` doesn't distinguish config validation errors from runtime init errors |

## Low Priority

| Crate | Issue |
|-------|-------|
| **rw-config** | All config structs have pub fields — prevents future validation |
| **rw-config** | Inconsistent: `DocsConfig::cache_dir()` is a getter but `project_dir` is a raw pub field |
| **rw-storage** | `StorageError::backend` field is public diagnostic info |
| **rw-storage-fs** | `FsStorage` uses 4+ constructor variants instead of a builder pattern |
| **rw-cache** | Empty `etag` string silently skips validation — undocumented behavior |
| **rw-napi** | `S3Config.entity` field semantics undocumented (Backstage terminology) |

## Clean Crates (no issues)

- **rw-assets** — minimal, clean 3-function API
- **rw** (CLI binary) — correctly uses `pub(crate)` throughout
