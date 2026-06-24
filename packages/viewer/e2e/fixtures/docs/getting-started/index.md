# Getting Started

This guide will help you get started with the platform.

## Overview

Follow these steps to begin:

1. Install the dependencies
2. Configure your environment
3. Run the application

## Next Steps

- [Installation Guide](./installation.md) - How to install
- [Configuration Guide](./configuration.md) - How to configure

## Tips

A few practices that tend to save time once your setup grows beyond the basics.

Keep your configuration under version control so changes are reviewable. Small,
focused commits make it easier to trace when a setting changed and why.

Prefer environment variables for secrets and per-machine values, and commit
only the defaults that are safe to share across every environment.

When something behaves unexpectedly, re-run with verbose logging before
changing configuration — the logs usually point straight at the cause.

Document any non-obvious setting next to where it is defined, so the next
reader does not have to reverse-engineer its purpose from behavior alone.
