# Feature notes

This page collects the GitHub-flavored constructs the renderer supports, so the
benchmark exercises their render paths.

## Alerts

> [!NOTE]
> Alerts render as styled callouts. This one is purely informational and is the
> most common kind you'll reach for.

> [!TIP]
> Combine the widget with a file watcher for a live-reload loop while you edit.

> [!WARNING]
> Changing the `--format` option after the first run invalidates the cache, so
> the next run is a full rebuild rather than an incremental one.

## Task lists

Roadmap for the next release:

- [x] Parse the new configuration format
- [x] Run transforms across parallel workers
- [ ] Stream incremental output as records finish
- [ ] ~~Ship the legacy XML format~~ (dropped)

## Inline extras

The old `--legacy` flag is ~~deprecated~~ removed — use `--format` instead. A
bare URL like https://example.com/widget is autolinked, as is an address such as
<team@example.com>, and inline `code spans` stay verbatim.

## Version comparison

| Feature     |  v1   |  v2 |
| :---------- | :---: | --: |
| Streaming   |  no   | yes |
| Parallelism |  no   | yes |
| Incremental |  no   | yes |
