# Plugin Development

Learn how to develop custom plugins.

## Overview

Plugins can extend the platform with:

- Custom commands
- New data sources
- UI components

## Creating a Plugin

See the [Custom Plugin Guide](./custom.md) for step-by-step instructions.

## Plugin API

```typescript
interface Plugin {
  name: string;
  version: string;
  init(): Promise<void>;
  destroy(): Promise<void>;
}
```
