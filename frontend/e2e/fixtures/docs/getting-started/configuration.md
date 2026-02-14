# Configuration

Learn how to configure the platform.

## Configuration File

Create a `config.toml` file in your project root:

```toml
[server]
host = "127.0.0.1"
port = 7979

[database]
url = "postgres://localhost/mydb"
pool_size = 10
```

## Environment Variables

You can also use environment variables:

| Variable       | Description                | Default     |
| -------------- | -------------------------- | ----------- |
| `HOST`         | Server host                | `127.0.0.1` |
| `PORT`         | Server port                | `7979`      |
| `DATABASE_URL` | Database connection string | -           |
| `LOG_LEVEL`    | Logging level              | `info`      |

## Configuration Priority

Configuration is loaded in this order:

1. Default values
2. Configuration file
3. Environment variables

Later sources override earlier ones.

## Next Steps

Return to the [Getting Started](./index.md) guide or go to [Installation](./installation.md).
