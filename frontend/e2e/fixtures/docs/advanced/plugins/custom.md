# Custom Plugin Guide

Step-by-step guide to creating a custom plugin.

## Prerequisites

Before you begin, ensure you have:

- Completed the [Installation](../../getting-started/installation.md)
- Read the [Plugin Development Overview](./index.md)

## Step 1: Create Plugin Structure

Create a new directory:

```bash
mkdir my-plugin
cd my-plugin
```

Create the plugin file:

```typescript
// my-plugin.ts
export default {
  name: "my-plugin",
  version: "1.0.0",

  async init() {
    console.log("Plugin initialized");
  },

  async destroy() {
    console.log("Plugin destroyed");
  },
};
```

## Step 2: Register Plugin

Register your plugin in the configuration:

```toml
[[plugins]]
name = "my-plugin"
path = "./plugins/my-plugin"
```

## Step 3: Test Plugin

Run the platform and verify your plugin loads:

```bash
platform serve --verbose
```

You should see "Plugin initialized" in the output.

## Next Steps

Return to the [Plugin Development](./index.md) overview or check out [Advanced Topics](../index.md).
