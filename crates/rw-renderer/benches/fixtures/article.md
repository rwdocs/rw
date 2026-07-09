---
title: Getting Started
description: A short guide to installing and running the widget.
---

# Getting Started

The **widget** is a small tool for *transforming* data. It reads input, applies
a set of rules, and writes the result. See the [reference](reference.md) for the
full option list, or run `widget --help` for a summary.

## Installation

Install the widget with your platform's package manager, then verify it:

1. Download the latest release for your platform.
2. Extract it to a directory on your `PATH`.
3. Confirm the install by running `widget --version`.

Once installed, you can run it against any input file without further setup.

## Configuration

The widget looks for a configuration file in the current directory or any parent
directory, walking upward until it finds one:

- `widget.toml` — the primary configuration file
- `.widgetignore` — patterns to skip during processing
  - one pattern per line
  - blank lines and lines starting with `#` are ignored
  - patterns are matched against the path relative to the config file

> Configuration is entirely optional. Without a config file the widget falls
> back to sensible defaults, so you can start with zero setup and introduce
> options only as you actually need them.

## How it works

Each run flows through the same three phases — parse, transform, emit — and the
widget reports a short summary when it finishes. Errors in a single record are
collected and reported at the end rather than aborting the whole run, so one bad
line never costs you the rest of the batch.

## Next steps

Read the [reference](reference.md) to learn about every option, or jump straight
into the [examples](examples.md) to see a few common setups end to end.
