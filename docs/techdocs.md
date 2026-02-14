# TechDocs

RW can build and publish static documentation sites compatible with [Backstage TechDocs](https://backstage.io/docs/features/techdocs/).

## Building

Generate a static site from your markdown documentation:

```bash
rw techdocs build --site-name "My Docs"
```

## Publishing to S3

Upload the built site to an S3-compatible storage bucket:

```bash
rw techdocs publish \
  --entity default/Component/my-service \
  --bucket my-techdocs-bucket \
  --endpoint https://storage.yandexcloud.net \
  --region ru-central1
```

## S3 Credentials

S3 credentials use standard `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` environment variables.

## Entity Format

The `--entity` flag uses the format `namespace/kind/name`, matching Backstage entity references (e.g., `default/Component/my-service`).
