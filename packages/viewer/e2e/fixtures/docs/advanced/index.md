# Advanced Topics

Advanced guides for power users.

## Topics

- [Custom Plugins](./plugins/index.md) - Extend functionality with plugins
- Performance optimization
- Security best practices

## Plugin System

The platform supports custom plugins. See the [Plugin Development](./plugins/index.md) guide.

## Performance Tips

1. Use caching where possible
2. Optimize database queries
3. Enable compression

## Scaling

Plan capacity ahead of demand. Horizontal scaling spreads load across
replicas; vertical scaling grows a single node. Most workloads start
vertical and graduate to horizontal once a single node is saturated.

Add read replicas for read-heavy traffic, shard for write-heavy traffic,
and keep a generous headroom margin so a traffic spike never lands you at
100% utilization with no slack to absorb it.

## Monitoring

Track the four golden signals — latency, traffic, errors, saturation —
and alert on symptoms users feel, not on every internal metric. A noisy
alert that never maps to user pain trains responders to ignore the pager.

## Comment guidelines

When reviewing changes, leave specific, actionable comments. This heading
exists so tests can verify that a heading whose slug starts with
`comment-` still deep-links as a normal heading anchor.
